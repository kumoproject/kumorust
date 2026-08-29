use std::io::Cursor;
use std::path::Path;

use icoextract_rs::{ExtractedIcon, IconExtractor};
use image::ImageFormat;
use windows::Win32::shellapi::ExtractIconExW;
use windows::Win32::windef::{HBITMAP, HGDIOBJ, HICON};
use windows::Win32::wingdi::{
    BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, DeleteObject, GetDIBits,
    GetObjectW,
};
use windows::Win32::winnt::HANDLE;
use windows::Win32::winuser::{DestroyIcon, GetDC, GetIconInfo, ReleaseDC};
use windows::core::PCWSTR;

pub fn try_extract_best_png(path: &Path) -> Option<Vec<u8>> {
    if !path.is_file() {
        return None;
    }

    try_extract_resource_png(path).or_else(|| try_extract_icon_png(path))
}

fn try_extract_resource_png(path: &Path) -> Option<Vec<u8>> {
    let icon = IconExtractor::from_path(path).ok()?.icon_by_index(0).ok()?;
    let frame = icon.images().iter().max_by_key(|image| {
        (
            u32::from(image.info().width()) * u32::from(image.info().height()),
            image.info().bit_count(),
            image.bytes().len(),
        )
    })?;
    let ico = ExtractedIcon::new(icon.resource_id().clone(), vec![frame.clone()])
        .to_ico_bytes()
        .ok()?;
    let image = image::load_from_memory_with_format(&ico, ImageFormat::Ico).ok()?;

    let mut png = Cursor::new(Vec::new());
    image.write_to(&mut png, ImageFormat::Png).ok()?;
    Some(png.into_inner())
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

    let result = (!icon_info.hbmColor.0.is_null())
        .then(|| extract_color_bitmap(icon_info.hbmColor))
        .flatten();

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

    let image = image::RgbaImage::from_raw(width, height, rgba)?;
    let mut encoded = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut encoded, ImageFormat::Png)
        .ok()?;
    Some(encoded.into_inner())
}
