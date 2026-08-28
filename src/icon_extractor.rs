use std::path::Path;

use windows::Win32::shellapi::ExtractIconExW;
use windows::Win32::windef::{HBITMAP, HGDIOBJ, HICON};
use windows::Win32::wingdi::{
    BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, DeleteObject, GetDIBits,
    GetObjectW,
};
use windows::Win32::winnt::HANDLE;
use windows::Win32::winuser::{DestroyIcon, GetDC, GetIconInfo, ReleaseDC};
use windows::core::PCWSTR;

const RT_ICON: u32 = 3;
const RT_GROUP_ICON: u32 = 14;

pub fn try_extract_best_png(path: &Path) -> Option<Vec<u8>> {
    if !path.is_file() {
        return None;
    }

    try_extract_raw_png(path).or_else(|| try_extract_icon_png(path))
}

fn try_extract_raw_png(path: &Path) -> Option<Vec<u8>> {
    let extractor = PeIconPngExtractor::new(path)?;
    extractor
        .extract_png_icons()
        .into_iter()
        .max_by_key(|icon| {
            (
                icon.width as u64 * icon.height as u64,
                icon.bit_count,
                icon.bytes.len(),
            )
        })
        .map(|icon| icon.bytes)
}

fn try_extract_icon_png(path: &Path) -> Option<Vec<u8>> {
    let wide_path: Vec<u16> = path
        .as_os_str()
        .to_string_lossy()
        .encode_utf16()
        .chain([0])
        .collect();
    let mut icon = HICON::default();

    let count = unsafe {
        ExtractIconExW(
            PCWSTR::from_raw(wide_path.as_ptr()),
            0,
            Some(&mut icon),
            None,
            1,
        )
    };
    if count == 0 || icon.0.is_null() {
        return None;
    }

    let result = extract_icon_bitmap(icon);
    unsafe {
        let _ = DestroyIcon(icon);
    }
    result
}

fn extract_icon_bitmap(icon: HICON) -> Option<Vec<u8>> {
    let mut icon_info = windows::Win32::winuser::ICONINFO::default();
    if unsafe { GetIconInfo(icon, &mut icon_info) }.0 == 0 {
        return None;
    }

    let result = if icon_info.hbmColor.0.is_null() {
        None
    } else {
        extract_color_bitmap(icon_info.hbmColor)
    };

    unsafe {
        if !icon_info.hbmColor.0.is_null() {
            let _ = DeleteObject(HGDIOBJ(icon_info.hbmColor.0));
        }
        if !icon_info.hbmMask.0.is_null() {
            let _ = DeleteObject(HGDIOBJ(icon_info.hbmMask.0));
        }
    }
    result
}

fn extract_color_bitmap(bitmap_handle: HBITMAP) -> Option<Vec<u8>> {
    let mut bitmap = BITMAP::default();
    let object_size = i32::try_from(std::mem::size_of::<BITMAP>()).ok()?;
    let object_result = unsafe {
        GetObjectW(
            HANDLE(bitmap_handle.0),
            object_size,
            Some((&mut bitmap as *mut BITMAP).cast()),
        )
    };
    if object_result <= 0 || bitmap.bmWidth <= 0 || bitmap.bmHeight <= 0 {
        return None;
    }

    let width = u32::try_from(bitmap.bmWidth).ok()?;
    let height = u32::try_from(bitmap.bmHeight).ok()?;
    if width > 4096 || height > 4096 {
        return None;
    }

    let pixel_count = usize::try_from(width)
        .ok()?
        .checked_mul(usize::try_from(height).ok()?)?;
    let mut bgra = vec![0_u8; pixel_count.checked_mul(4)?];
    let mut info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: u32::try_from(std::mem::size_of::<BITMAPINFOHEADER>()).ok()?,
            biWidth: bitmap.bmWidth,
            biHeight: -bitmap.bmHeight,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB as u32,
            ..Default::default()
        },
        ..Default::default()
    };

    let hdc = unsafe { GetDC(None) };
    if hdc.0.is_null() {
        return None;
    }
    let copied = unsafe {
        GetDIBits(
            hdc,
            bitmap_handle,
            0,
            height,
            Some(bgra.as_mut_ptr().cast()),
            &mut info,
            DIB_RGB_COLORS as u32,
        )
    };
    unsafe {
        let _ = ReleaseDC(None, hdc);
    }
    if copied <= 0 {
        return None;
    }

    let mut rgba = Vec::with_capacity(bgra.len());
    for pixel in bgra.chunks_exact(4) {
        let alpha = if pixel[3] == 0 && (pixel[0] != 0 || pixel[1] != 0 || pixel[2] != 0) {
            255
        } else {
            pixel[3]
        };
        rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], alpha]);
    }

    let mut encoded = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut encoded, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().ok()?;
        writer.write_image_data(&rgba).ok()?;
        writer.finish().ok()?;
    }
    Some(encoded)
}

struct PeIconPngExtractor {
    bytes: Vec<u8>,
    sections: Vec<Section>,
    resource_directory_offset: usize,
}

impl PeIconPngExtractor {
    fn new(path: &Path) -> Option<Self> {
        let bytes = std::fs::read(path).ok()?;
        let (sections, resource_rva) = read_pe_headers(&bytes)?;
        let resource_directory_offset = rva_to_file_offset(&sections, resource_rva, bytes.len())?;
        Some(Self {
            bytes,
            sections,
            resource_directory_offset,
        })
    }

    fn extract_png_icons(&self) -> Vec<PngIcon> {
        let icon_resources = self
            .enumerate_resource_data(RT_ICON)
            .into_iter()
            .filter_map(|resource| Some((resource.name_id?, resource.bytes)))
            .collect::<std::collections::HashMap<_, _>>();
        let mut icons = Vec::new();

        for group_resource in self.enumerate_resource_data(RT_GROUP_ICON) {
            parse_icon_group(&group_resource.bytes, &icon_resources, &mut icons);
        }
        icons
    }

    fn enumerate_resource_data(&self, resource_type: u32) -> Vec<ResourceData> {
        let Some(type_entry) = self
            .read_directory_entries(self.resource_directory_offset)
            .into_iter()
            .find(|entry| !entry.is_named && entry.id == resource_type && entry.is_directory)
        else {
            return Vec::new();
        };
        let Some(directory_offset) = self
            .resource_directory_offset
            .checked_add(type_entry.offset)
        else {
            return Vec::new();
        };

        let mut result = Vec::new();
        self.walk_resource_directory(directory_offset, None, 0, &mut result);
        result
    }

    fn walk_resource_directory(
        &self,
        directory_offset: usize,
        name_id: Option<u32>,
        depth: u8,
        result: &mut Vec<ResourceData>,
    ) {
        if depth > 3 {
            return;
        }
        for entry in self.read_directory_entries(directory_offset) {
            let next_name_id = if depth == 0 && !entry.is_named {
                Some(entry.id)
            } else {
                name_id
            };
            let Some(entry_offset) = self.resource_directory_offset.checked_add(entry.offset)
            else {
                continue;
            };

            if entry.is_directory {
                self.walk_resource_directory(entry_offset, next_name_id, depth + 1, result);
                continue;
            }

            let Some(data_rva) = read_u32(&self.bytes, entry_offset) else {
                continue;
            };
            let Some(size_offset) = entry_offset.checked_add(4) else {
                continue;
            };
            let Some(size) = read_u32(&self.bytes, size_offset) else {
                continue;
            };
            let Some(data_offset) = rva_to_file_offset(&self.sections, data_rva, self.bytes.len())
            else {
                continue;
            };
            let Some(size) = usize::try_from(size).ok() else {
                continue;
            };
            let Some(data_end) = data_offset.checked_add(size) else {
                continue;
            };
            let Some(bytes) = self.bytes.get(data_offset..data_end) else {
                continue;
            };
            result.push(ResourceData {
                name_id: next_name_id,
                bytes: bytes.to_vec(),
            });
        }
    }

    fn read_directory_entries(&self, directory_offset: usize) -> Vec<ResourceDirectoryEntry> {
        let Some(named_offset) = directory_offset.checked_add(12) else {
            return Vec::new();
        };
        let Some(named_count) = read_u16(&self.bytes, named_offset) else {
            return Vec::new();
        };
        let Some(id_offset) = directory_offset.checked_add(14) else {
            return Vec::new();
        };
        let Some(id_count) = read_u16(&self.bytes, id_offset) else {
            return Vec::new();
        };
        let Some(total_count) = usize::from(named_count).checked_add(usize::from(id_count)) else {
            return Vec::new();
        };

        (0..total_count)
            .filter_map(|index| {
                let entry_offset = directory_offset
                    .checked_add(16)?
                    .checked_add(index.checked_mul(8)?)?;
                let name_or_id = read_u32(&self.bytes, entry_offset)?;
                let data_or_directory = read_u32(&self.bytes, entry_offset.checked_add(4)?)?;
                Some(ResourceDirectoryEntry {
                    is_named: name_or_id & 0x8000_0000 != 0,
                    id: name_or_id & 0xFFFF,
                    is_directory: data_or_directory & 0x8000_0000 != 0,
                    offset: usize::try_from(data_or_directory & 0x7FFF_FFFF).ok()?,
                })
            })
            .collect()
    }
}

fn parse_icon_group(
    group_bytes: &[u8],
    icon_resources: &std::collections::HashMap<u32, Vec<u8>>,
    result: &mut Vec<PngIcon>,
) {
    let (Some(reserved), Some(icon_type), Some(count)) = (
        read_u16(group_bytes, 0),
        read_u16(group_bytes, 2),
        read_u16(group_bytes, 4),
    ) else {
        return;
    };
    if reserved != 0 || icon_type != 1 {
        return;
    }
    let Some(entries_size) = usize::from(count).checked_mul(14) else {
        return;
    };
    let Some(entries_end) = 6usize.checked_add(entries_size) else {
        return;
    };
    if group_bytes.len() < entries_end {
        return;
    }

    for index in 0..usize::from(count) {
        let Some(offset) = 6usize.checked_add(index.checked_mul(14).unwrap_or(0)) else {
            continue;
        };
        let Some(icon_id) = read_u16(group_bytes, offset + 12) else {
            continue;
        };
        let Some(bytes) = icon_resources.get(&u32::from(icon_id)) else {
            continue;
        };
        let Some((width, height, bit_count)) = read_png_info(bytes) else {
            continue;
        };
        result.push(PngIcon {
            width,
            height,
            bit_count,
            bytes: bytes.clone(),
        });
    }
}

fn read_pe_headers(bytes: &[u8]) -> Option<(Vec<Section>, u32)> {
    if read_u16(bytes, 0)? != 0x5A4D {
        return None;
    }
    let pe_offset = usize::try_from(read_u32(bytes, 0x3C)?).ok()?;
    if read_u32(bytes, pe_offset)? != 0x0000_4550 {
        return None;
    }

    let coff_offset = pe_offset.checked_add(4)?;
    let section_count = usize::from(read_u16(bytes, coff_offset.checked_add(2)?)?);
    let optional_header_size = usize::from(read_u16(bytes, coff_offset.checked_add(16)?)?);
    let optional_header_offset = coff_offset.checked_add(20)?;
    let magic = read_u16(bytes, optional_header_offset)?;
    let data_directory_offset = match magic {
        0x10B => optional_header_offset.checked_add(96)?,
        0x20B => optional_header_offset.checked_add(112)?,
        _ => return None,
    };
    let resource_directory_rva = read_u32(bytes, data_directory_offset.checked_add(16)?)?;
    if resource_directory_rva == 0 {
        return None;
    }

    let section_table_offset = optional_header_offset.checked_add(optional_header_size)?;
    let sections = (0..section_count)
        .filter_map(|index| {
            let section_offset = section_table_offset.checked_add(index.checked_mul(40)?)?;
            Some(Section {
                virtual_size: read_u32(bytes, section_offset.checked_add(8)?)?,
                virtual_address: read_u32(bytes, section_offset.checked_add(12)?)?,
                raw_data_size: read_u32(bytes, section_offset.checked_add(16)?)?,
                raw_data_pointer: read_u32(bytes, section_offset.checked_add(20)?)?,
            })
        })
        .collect::<Vec<_>>();
    (sections.len() == section_count).then_some((sections, resource_directory_rva))
}

fn rva_to_file_offset(sections: &[Section], rva: u32, file_length: usize) -> Option<usize> {
    let rva = u64::from(rva);
    for section in sections {
        let section_size = u64::from(section.virtual_size.max(section.raw_data_size));
        let start = u64::from(section.virtual_address);
        let end = start.checked_add(section_size)?;
        if rva < start || rva >= end {
            continue;
        }
        let offset = u64::from(section.raw_data_pointer).checked_add(rva - start)?;
        let offset = usize::try_from(offset).ok()?;
        return offset
            .checked_add(1)
            .filter(|end| *end <= file_length)
            .map(|_| offset);
    }
    None
}

fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes([
        *bytes.get(offset)?,
        *bytes.get(offset.checked_add(1)?)?,
    ]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes([
        *bytes.get(offset)?,
        *bytes.get(offset.checked_add(1)?)?,
        *bytes.get(offset.checked_add(2)?)?,
        *bytes.get(offset.checked_add(3)?)?,
    ]))
}

fn read_png_info(bytes: &[u8]) -> Option<(u32, u32, u16)> {
    const PNG_SIGNATURE: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    if !bytes.starts_with(&PNG_SIGNATURE) || bytes.len() < 33 {
        return None;
    }
    let width = u32::from_be_bytes(bytes[16..20].try_into().ok()?);
    let height = u32::from_be_bytes(bytes[20..24].try_into().ok()?);
    let bit_depth = u16::from(bytes[24]);
    let channels = match bytes[25] {
        0 | 3 => 1,
        2 => 3,
        4 => 2,
        6 => 4,
        _ => return None,
    };
    (width > 0 && height > 0).then_some((width, height, bit_depth * channels))
}

struct Section {
    virtual_size: u32,
    virtual_address: u32,
    raw_data_size: u32,
    raw_data_pointer: u32,
}

struct ResourceDirectoryEntry {
    is_named: bool,
    id: u32,
    is_directory: bool,
    offset: usize,
}

struct ResourceData {
    name_id: Option<u32>,
    bytes: Vec<u8>,
}

struct PngIcon {
    width: u32,
    height: u32,
    bit_count: u16,
    bytes: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::{Section, read_png_info, rva_to_file_offset};

    #[test]
    fn reads_png_dimensions_and_bit_depth() {
        let mut bytes = vec![0_u8; 33];
        bytes[..8].copy_from_slice(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);
        bytes[16..20].copy_from_slice(&256_u32.to_be_bytes());
        bytes[20..24].copy_from_slice(&128_u32.to_be_bytes());
        bytes[24] = 8;
        bytes[25] = 6;

        assert_eq!(read_png_info(&bytes), Some((256, 128, 32)));
    }

    #[test]
    fn maps_resource_rva_into_a_section() {
        let sections = [Section {
            virtual_size: 0x200,
            virtual_address: 0x1000,
            raw_data_size: 0x200,
            raw_data_pointer: 0x400,
        }];
        assert_eq!(rva_to_file_offset(&sections, 0x1050, 0x800), Some(0x450));
        assert_eq!(rva_to_file_offset(&sections, 0x1300, 0x800), None);
    }
}
