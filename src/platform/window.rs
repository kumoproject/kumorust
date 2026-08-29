use core::ffi::c_void;
use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};

use crate::domain::settings::{self, CloseBehavior};
use windows::Foundation::TypedEventHandler;
use windows::Win32::{
    commctrl::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
    minwindef::{LPARAM, LRESULT, WPARAM},
    windef::HWND,
    winuser::{
        FindWindowW, IsWindow, PostMessageW, SW_HIDE, SW_RESTORE, SetForegroundWindow, ShowWindow,
        WM_APP, WM_CLOSE, WM_NCDESTROY,
    },
};
use windows::core::{
    GUID, HRESULT, HSTRING, IInspectable_Vtbl, IUnknown, IUnknown_Vtbl, Interface, PCWSTR, Ref,
    RuntimeName, RuntimeType,
};
use windows_reactor::{
    Backdrop, Element, ReactorWindow, RenderCompleteInfo, RenderCx, WindowHandle, with_active_host,
};

#[repr(transparent)]
#[derive(Clone, Debug, Eq, PartialEq)]
struct IWindowNative(IUnknown);

unsafe impl Interface for IWindowNative {
    type Vtable = IWindowNativeVtbl;
    const IID: GUID = GUID::from_u128(0xeecdbf0e_bae9_4cb6_a68e_9598e1cb57bb);
}

#[repr(C)]
struct IWindowNativeVtbl {
    base__: IUnknown_Vtbl,
    window_handle: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> HRESULT,
}

#[repr(transparent)]
#[derive(Clone, Debug, Eq, PartialEq)]
struct IWindow2(IUnknown);

unsafe impl Interface for IWindow2 {
    type Vtable = IWindow2Vtbl;
    const IID: GUID = GUID::from_u128(0x42febaa5_1c32_522a_a591_57618c6f665d);
}

#[repr(C)]
struct IWindow2Vtbl {
    base__: IInspectable_Vtbl,
    system_backdrop: usize,
    set_system_backdrop: unsafe extern "system" fn(*mut c_void, *mut c_void) -> HRESULT,
    app_window: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> HRESULT,
}

impl IWindow2 {
    fn app_window(&self) -> windows::core::Result<IUnknown> {
        unsafe {
            let mut result = core::ptr::null_mut();
            (self.vtable().app_window)(self.as_raw(), &mut result).ok()?;
            if result.is_null() {
                return Err(windows::core::Error::empty());
            }
            Ok(<IUnknown as Interface>::from_raw(result))
        }
    }
}

#[repr(transparent)]
#[derive(Clone, Debug, Eq, PartialEq)]
struct AppWindow(IUnknown);

unsafe impl Interface for AppWindow {
    type Vtable = AppWindowVtbl;
    const IID: GUID = GUID::from_u128(0xcfa788b3_643b_5c5e_ad4e_321d48a82acd);
}

#[repr(C)]
struct AppWindowVtbl {
    base__: IInspectable_Vtbl,
    methods_before_title_bar: [usize; 10],
    title_bar: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> HRESULT,
}

impl AppWindow {
    fn title_bar(&self) -> windows::core::Result<IUnknown> {
        unsafe {
            let mut result = core::ptr::null_mut();
            (self.vtable().title_bar)(self.as_raw(), &mut result).ok()?;
            if result.is_null() {
                return Err(windows::core::Error::empty());
            }
            Ok(<IUnknown as Interface>::from_raw(result))
        }
    }
}

#[repr(transparent)]
#[derive(Clone, Debug, Eq, PartialEq)]
struct AppWindowTitleBar(IUnknown);

unsafe impl Interface for AppWindowTitleBar {
    type Vtable = AppWindowTitleBarVtbl;
    const IID: GUID = GUID::from_u128(0x5574efa2_c91c_5700_a363_539c71a7aaf4);
}

#[repr(C)]
struct AppWindowTitleBarVtbl {
    base__: IInspectable_Vtbl,
    methods_before_icon_show_options: [usize; 24],
    set_icon_show_options: unsafe extern "system" fn(*mut c_void, i32) -> HRESULT,
}

static MAIN_HWND: AtomicIsize = AtomicIsize::new(0);
static CLOSE_SUBCLASS_INSTALLED: AtomicBool = AtomicBool::new(false);
static CLOSE_DIALOG_OPEN: AtomicBool = AtomicBool::new(false);
static ALLOW_CLOSE: AtomicBool = AtomicBool::new(false);

#[repr(transparent)]
#[derive(Clone, Debug, Eq, PartialEq)]
struct ContentDialog(IUnknown);

unsafe impl Interface for ContentDialog {
    type Vtable = IContentDialogVtbl;
    const IID: GUID = GUID::from_u128(0xac2145a3_4a32_5305_a81d_47509515bfce);
}

impl RuntimeType for ContentDialog {
    const SIGNATURE: windows::core::imp::ConstBuffer =
        windows::core::imp::ConstBuffer::for_class::<Self, IContentDialog>();
}

impl RuntimeName for ContentDialog {
    const NAME: &'static str = "Microsoft.UI.Xaml.Controls.ContentDialog";
}

unsafe impl Send for ContentDialog {}
unsafe impl Sync for ContentDialog {}

#[repr(transparent)]
#[derive(Clone, Debug, Eq, PartialEq)]
struct ContentDialogClosedEventArgs(IUnknown);

unsafe impl Interface for ContentDialogClosedEventArgs {
    type Vtable = IContentDialogClosedEventArgsVtbl;
    const IID: GUID = GUID::from_u128(0x9b84e681_1ab6_5485_88b2_d0d3c05b29f3);
}

impl RuntimeType for ContentDialogClosedEventArgs {
    const SIGNATURE: windows::core::imp::ConstBuffer =
        windows::core::imp::ConstBuffer::for_class::<Self, IContentDialogClosedEventArgs>();
}

impl RuntimeName for ContentDialogClosedEventArgs {
    const NAME: &'static str = "Microsoft.UI.Xaml.Controls.ContentDialogClosedEventArgs";
}

unsafe impl Send for ContentDialogClosedEventArgs {}
unsafe impl Sync for ContentDialogClosedEventArgs {}

#[repr(transparent)]
#[derive(Clone, Debug, Eq, PartialEq)]
struct CheckBox(IUnknown);

unsafe impl Interface for CheckBox {
    type Vtable = ICheckBoxVtbl;
    const IID: GUID = GUID::from_u128(0xc5830000_4c9d_5fdd_9346_674c71cd80c5);
}

impl RuntimeType for CheckBox {
    const SIGNATURE: windows::core::imp::ConstBuffer =
        windows::core::imp::ConstBuffer::for_class::<Self, ICheckBox>();
}

impl RuntimeName for CheckBox {
    const NAME: &'static str = "Microsoft.UI.Xaml.Controls.CheckBox";
}

unsafe impl Send for CheckBox {}
unsafe impl Sync for CheckBox {}

#[repr(transparent)]
#[derive(Clone, Debug, Eq, PartialEq)]
struct IContentDialog(IUnknown);

unsafe impl Interface for IContentDialog {
    type Vtable = IContentDialogVtbl;
    const IID: GUID = GUID::from_u128(0xac2145a3_4a32_5305_a81d_47509515bfce);
}

impl RuntimeType for IContentDialog {
    const SIGNATURE: windows::core::imp::ConstBuffer =
        windows::core::imp::ConstBuffer::for_interface::<Self>();
}

#[repr(C)]
struct IContentDialogVtbl {
    base__: IInspectable_Vtbl,
    title: usize,
    set_title: unsafe extern "system" fn(*mut c_void, *mut c_void) -> HRESULT,
    title_template: usize,
    set_title_template: usize,
    full_size_desired: usize,
    set_full_size_desired: usize,
    primary_button_text: usize,
    set_primary_button_text: unsafe extern "system" fn(*mut c_void, *mut c_void) -> HRESULT,
    secondary_button_text: usize,
    set_secondary_button_text: unsafe extern "system" fn(*mut c_void, *mut c_void) -> HRESULT,
    close_button_text: usize,
    set_close_button_text: unsafe extern "system" fn(*mut c_void, *mut c_void) -> HRESULT,
    methods_before_enabled_buttons: [usize; 12],
    is_primary_button_enabled: usize,
    set_is_primary_button_enabled: unsafe extern "system" fn(*mut c_void, bool) -> HRESULT,
    is_secondary_button_enabled: usize,
    set_is_secondary_button_enabled: unsafe extern "system" fn(*mut c_void, bool) -> HRESULT,
    methods_before_closed: [usize; 10],
    closed: unsafe extern "system" fn(*mut c_void, *mut c_void, *mut i64) -> HRESULT,
    remove_closed: unsafe extern "system" fn(*mut c_void, i64) -> HRESULT,
    methods_before_hide: [usize; 8],
    hide: unsafe extern "system" fn(*mut c_void) -> HRESULT,
    show_async: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> HRESULT,
}

impl IContentDialog {
    fn set_title<T: Interface>(&self, value: &T) -> windows::core::Result<()> {
        unsafe { (self.vtable().set_title)(self.as_raw(), value.as_raw()).ok() }
    }

    fn set_button_text(
        &self,
        setter: unsafe extern "system" fn(*mut c_void, *mut c_void) -> HRESULT,
        value: &str,
    ) -> windows::core::Result<()> {
        let value = HSTRING::from(value);
        unsafe { setter(self.as_raw(), core::mem::transmute_copy(&value)).ok() }
    }

    fn set_primary_button_text(&self, value: &str) -> windows::core::Result<()> {
        self.set_button_text(self.vtable().set_primary_button_text, value)
    }

    fn set_secondary_button_text(&self, value: &str) -> windows::core::Result<()> {
        self.set_button_text(self.vtable().set_secondary_button_text, value)
    }

    fn set_close_button_text(&self, value: &str) -> windows::core::Result<()> {
        self.set_button_text(self.vtable().set_close_button_text, value)
    }

    fn closed<F>(&self, handler: F) -> windows::core::Result<i64>
    where
        F: Fn(Ref<ContentDialog>, Ref<ContentDialogClosedEventArgs>) -> windows::core::Result<()>
            + Send
            + 'static,
    {
        let handler =
            TypedEventHandler::<ContentDialog, ContentDialogClosedEventArgs>::new(handler);
        unsafe {
            let mut token = 0;
            (self.vtable().closed)(self.as_raw(), handler.as_raw(), &mut token).ok()?;
            Ok(token)
        }
    }

    fn show_async(&self) -> windows::core::Result<IUnknown> {
        unsafe {
            let mut result = core::ptr::null_mut();
            (self.vtable().show_async)(self.as_raw(), &mut result).ok()?;
            if result.is_null() {
                return Err(windows::core::Error::empty());
            }
            Ok(IUnknown::from_raw(result))
        }
    }
}

#[repr(transparent)]
#[derive(Clone, Debug, Eq, PartialEq)]
struct IContentDialogClosedEventArgs(IUnknown);

unsafe impl Interface for IContentDialogClosedEventArgs {
    type Vtable = IContentDialogClosedEventArgsVtbl;
    const IID: GUID = GUID::from_u128(0x9b84e681_1ab6_5485_88b2_d0d3c05b29f3);
}

impl RuntimeType for IContentDialogClosedEventArgs {
    const SIGNATURE: windows::core::imp::ConstBuffer =
        windows::core::imp::ConstBuffer::for_interface::<Self>();
}

#[repr(C)]
struct IContentDialogClosedEventArgsVtbl {
    base__: IInspectable_Vtbl,
    result: unsafe extern "system" fn(*mut c_void, *mut i32) -> HRESULT,
}

impl IContentDialogClosedEventArgs {
    fn result(&self) -> windows::core::Result<i32> {
        unsafe {
            let mut result = 0;
            (self.vtable().result)(self.as_raw(), &mut result).ok()?;
            Ok(result)
        }
    }
}

#[repr(transparent)]
#[derive(Clone, Debug, Eq, PartialEq)]
struct ICheckBox(IUnknown);

unsafe impl Interface for ICheckBox {
    type Vtable = ICheckBoxVtbl;
    const IID: GUID = GUID::from_u128(0xc5830000_4c9d_5fdd_9346_674c71cd80c5);
}

impl RuntimeType for ICheckBox {
    const SIGNATURE: windows::core::imp::ConstBuffer =
        windows::core::imp::ConstBuffer::for_interface::<Self>();
}

#[repr(C)]
struct ICheckBoxVtbl {
    base__: IInspectable_Vtbl,
}

#[repr(transparent)]
#[derive(Clone, Debug, Eq, PartialEq)]
struct IContentControl(IUnknown);

unsafe impl Interface for IContentControl {
    type Vtable = IContentControlVtbl;
    const IID: GUID = GUID::from_u128(0x07e81761_11b2_52ae_8f8b_4d53d2b5900a);
}

#[repr(C)]
struct IContentControlVtbl {
    base__: IInspectable_Vtbl,
    content: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> HRESULT,
    set_content: unsafe extern "system" fn(*mut c_void, *mut c_void) -> HRESULT,
}

impl IContentControl {
    fn set_content<T: Interface>(&self, value: &T) -> windows::core::Result<()> {
        unsafe { (self.vtable().set_content)(self.as_raw(), value.as_raw()).ok() }
    }
}

#[repr(transparent)]
#[derive(Clone, Debug, Eq, PartialEq)]
struct IUIElement(IUnknown);

unsafe impl Interface for IUIElement {
    type Vtable = IUIElementVtbl;
    const IID: GUID = GUID::from_u128(0xc3c01020_320c_5cf6_9d24_d396bbfa4d8b);
}

#[repr(C)]
struct IUIElementVtbl {
    base__: IInspectable_Vtbl,
    methods_before_xaml_root: [usize; 103],
    xaml_root: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> HRESULT,
    set_xaml_root: unsafe extern "system" fn(*mut c_void, *mut c_void) -> HRESULT,
}

impl IUIElement {
    fn xaml_root(&self) -> windows::core::Result<IUnknown> {
        unsafe {
            let mut result = core::ptr::null_mut();
            (self.vtable().xaml_root)(self.as_raw(), &mut result).ok()?;
            if result.is_null() {
                return Err(windows::core::Error::empty());
            }
            Ok(IUnknown::from_raw(result))
        }
    }

    fn set_xaml_root<T: Interface>(&self, value: &T) -> windows::core::Result<()> {
        unsafe { (self.vtable().set_xaml_root)(self.as_raw(), value.as_raw()).ok() }
    }
}

#[repr(transparent)]
#[derive(Clone, Debug, Eq, PartialEq)]
struct IToggleButton(IUnknown);

unsafe impl Interface for IToggleButton {
    type Vtable = IToggleButtonVtbl;
    const IID: GUID = GUID::from_u128(0x686fbaa4_c866_568b_8f75_481d8d545291);
}

#[repr(C)]
struct IToggleButtonVtbl {
    base__: IInspectable_Vtbl,
    is_checked: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> HRESULT,
    set_is_checked: unsafe extern "system" fn(*mut c_void, *mut c_void) -> HRESULT,
}

impl IToggleButton {
    fn is_checked(&self) -> bool {
        unsafe {
            let mut result = core::ptr::null_mut();
            if (self.vtable().is_checked)(self.as_raw(), &mut result).is_err() || result.is_null() {
                return false;
            }
            windows_reference::IReference::<bool>::from_raw(result)
                .Value()
                .unwrap_or(false)
        }
    }
}

#[repr(transparent)]
#[derive(Clone, Debug, Eq, PartialEq)]
struct IWindow(IUnknown);

unsafe impl Interface for IWindow {
    type Vtable = IWindowVtbl;
    const IID: GUID = GUID::from_u128(0x61f0ec79_5d52_56b5_86fb_40fa4af288b0);
}

#[repr(C)]
struct IWindowVtbl {
    base__: IInspectable_Vtbl,
    bounds: usize,
    visible: usize,
    content: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> HRESULT,
}

impl IWindow {
    fn content(&self) -> windows::core::Result<IUnknown> {
        unsafe {
            let mut result = core::ptr::null_mut();
            (self.vtable().content)(self.as_raw(), &mut result).ok()?;
            if result.is_null() {
                return Err(windows::core::Error::empty());
            }
            Ok(IUnknown::from_raw(result))
        }
    }
}

#[repr(C)]
struct IContentDialogFactoryVtbl {
    base__: IInspectable_Vtbl,
    create_instance: unsafe extern "system" fn(
        *mut c_void,
        *mut c_void,
        *mut *mut c_void,
        *mut *mut c_void,
    ) -> HRESULT,
}

#[repr(transparent)]
#[derive(Clone, Debug, Eq, PartialEq)]
struct IContentDialogFactory(IUnknown);

unsafe impl Interface for IContentDialogFactory {
    type Vtable = IContentDialogFactoryVtbl;
    const IID: GUID = GUID::from_u128(0xa05b3ad7_c60e_545a_9ee4_f098220ed816);
}

#[repr(C)]
struct ICheckBoxFactoryVtbl {
    base__: IInspectable_Vtbl,
    create_instance: unsafe extern "system" fn(
        *mut c_void,
        *mut c_void,
        *mut *mut c_void,
        *mut *mut c_void,
    ) -> HRESULT,
}

impl ContentDialog {
    fn new() -> windows::core::Result<Self> {
        let factory = windows::core::factory::<Self, IContentDialogFactory>()?;
        unsafe {
            let mut result = core::ptr::null_mut();
            (factory.vtable().create_instance)(
                factory.as_raw(),
                core::ptr::null_mut(),
                core::ptr::null_mut(),
                &mut result,
            )
            .ok()?;
            if result.is_null() {
                return Err(windows::core::Error::empty());
            }
            Ok(Self::from_raw(result))
        }
    }
}

impl CheckBox {
    fn new() -> windows::core::Result<Self> {
        let factory = windows::core::factory::<Self, ICheckBoxFactory>()?;
        unsafe {
            let mut result = core::ptr::null_mut();
            (factory.vtable().create_instance)(
                factory.as_raw(),
                core::ptr::null_mut(),
                core::ptr::null_mut(),
                &mut result,
            )
            .ok()?;
            if result.is_null() {
                return Err(windows::core::Error::empty());
            }
            Ok(Self::from_raw(result))
        }
    }
}

#[repr(transparent)]
#[derive(Clone, Debug, Eq, PartialEq)]
struct ICheckBoxFactory(IUnknown);

unsafe impl Interface for ICheckBoxFactory {
    type Vtable = ICheckBoxFactoryVtbl;
    const IID: GUID = GUID::from_u128(0xf43ff58d_31d5_5835_af7b_375bc6a9bcf3);
}

struct CloseDialogState {
    _dialog: ContentDialog,
    _checkbox: CheckBox,
    _operation: Option<IUnknown>,
}

thread_local! {
    static CLOSE_DIALOG: RefCell<Option<CloseDialogState>> = const { RefCell::new(None) };
    static KEEPALIVE_WINDOW: RefCell<Option<WindowHandle>> = const { RefCell::new(None) };
}

const CLOSE_SUBCLASS_ID: usize = 0x4b;
const SHOW_CLOSE_MESSAGE: u32 = (WM_APP + 1) as u32;
const KEEPALIVE_WINDOW_TITLE: &str = "KumoRust.keepalive";
pub(crate) const MAIN_WINDOW_TITLE: &str = "kumokumo";

pub(crate) fn ensure_keepalive_window() {
    if KEEPALIVE_WINDOW.with(|slot| slot.borrow().is_some()) {
        return;
    }

    let Ok(handle) = ReactorWindow::new()
        .title(KEEPALIVE_WINDOW_TITLE)
        .inner_size(1.0, 1.0)
        .render(keepalive_root)
    else {
        return;
    };

    KEEPALIVE_WINDOW.with(|slot| *slot.borrow_mut() = Some(handle));
}

fn keepalive_root(cx: &mut RenderCx) -> Element {
    cx.use_effect((), hide_keepalive_window);
    Element::Empty
}

fn hide_keepalive_window() {
    if let Some(hwnd) = find_window(KEEPALIVE_WINDOW_TITLE) {
        unsafe {
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
    }
}

fn release_keepalive_window() {
    let handle = KEEPALIVE_WINDOW.with(|slot| slot.borrow_mut().take());
    if let Some(handle) = handle {
        handle.close();
    }
}

pub fn install_titlebar_icon_hider() {
    let _ = with_active_host(|host| {
        let window = host.window().clone();
        remember_window_hwnd(&window);
        install_close_subclass(&window);
        host.set_render_complete(move |_info: &RenderCompleteInfo| {
            remember_window_hwnd(&window);
            install_close_subclass(&window);
            hide_titlebar_icon(&window);
        });
    });
}

pub(crate) fn exit_application() {
    release_keepalive_window();

    let raw = MAIN_HWND.load(Ordering::Acquire);
    if raw == 0 || !post_allowed_close(HWND(raw as *mut c_void)) {
        std::process::exit(0);
    }
}

pub(crate) fn activate_main_window() {
    let raw = MAIN_HWND.load(Ordering::Acquire);
    if raw != 0 {
        let hwnd = HWND(raw as *mut c_void);
        if unsafe { IsWindow(Some(hwnd)).as_bool() } {
            unsafe {
                let _ = ShowWindow(hwnd, SW_RESTORE);
                let _ = SetForegroundWindow(hwnd);
            }
            return;
        }
        let _ = MAIN_HWND.compare_exchange(raw, 0, Ordering::AcqRel, Ordering::Acquire);
    }

    if ReactorWindow::new()
        .title(MAIN_WINDOW_TITLE)
        .backdrop(Backdrop::Mica)
        .render(crate::app::app)
        .is_ok()
    {
        // Open the replacement before closing the keepalive window, otherwise
        // the reactor sees no windows and terminates the process.
        release_keepalive_window();
    }
}

pub(crate) fn activate_existing_main_window() {
    if let Some(hwnd) = find_window(MAIN_WINDOW_TITLE) {
        unsafe {
            let _ = ShowWindow(hwnd, SW_RESTORE);
            let _ = SetForegroundWindow(hwnd);
        }
    }
}

fn find_window(title: &str) -> Option<HWND> {
    let title = title.encode_utf16().chain([0]).collect::<Vec<_>>();
    let hwnd = unsafe { FindWindowW(None, PCWSTR::from_raw(title.as_ptr())) };
    (!hwnd.0.is_null()).then_some(hwnd)
}

fn remember_window_hwnd(window: &impl Interface) {
    if let Some(hwnd) = window_hwnd(window) {
        MAIN_HWND.store(hwnd.0 as isize, Ordering::Release);
    }
}

fn window_hwnd(window: &impl Interface) -> Option<HWND> {
    let native = window.cast::<IWindowNative>().ok()?;
    let mut raw = core::ptr::null_mut();
    unsafe {
        (native.vtable().window_handle)(native.as_raw(), &mut raw)
            .ok()
            .ok()?;
    }
    (!raw.is_null()).then_some(HWND(raw))
}

fn install_close_subclass(window: &impl Interface) {
    let Some(hwnd) = window_hwnd(window) else {
        return;
    };
    if CLOSE_SUBCLASS_INSTALLED
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return;
    }

    let installed = unsafe {
        SetWindowSubclass(hwnd, Some(close_subclass_proc), CLOSE_SUBCLASS_ID, 0).as_bool()
    };
    if !installed {
        CLOSE_SUBCLASS_INSTALLED.store(false, Ordering::Release);
    }
}

unsafe extern "system" fn close_subclass_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _subclass_id: usize,
    _ref_data: usize,
) -> LRESULT {
    if message == WM_NCDESTROY as u32 {
        let _ = MAIN_HWND.compare_exchange(hwnd.0 as isize, 0, Ordering::AcqRel, Ordering::Acquire);
        CLOSE_SUBCLASS_INSTALLED.store(false, Ordering::Release);
        CLOSE_DIALOG_OPEN.store(false, Ordering::Release);
        ALLOW_CLOSE.store(false, Ordering::Release);
        unsafe {
            let _ = RemoveWindowSubclass(hwnd, Some(close_subclass_proc), CLOSE_SUBCLASS_ID);
            return DefSubclassProc(hwnd, message, wparam, lparam);
        }
    }

    if message == SHOW_CLOSE_MESSAGE {
        handle_close_request(hwnd);
        return LRESULT(0);
    }

    if message == WM_CLOSE as u32 && !ALLOW_CLOSE.swap(false, Ordering::AcqRel) {
        unsafe {
            let _ = PostMessageW(Some(hwnd), SHOW_CLOSE_MESSAGE, WPARAM(0), LPARAM(0));
        }
        return LRESULT(0);
    }

    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}

fn handle_close_request(hwnd: HWND) {
    if CLOSE_DIALOG_OPEN.swap(true, Ordering::AcqRel) {
        return;
    }

    if let Some(behavior) = settings::load_close_behavior() {
        CLOSE_DIALOG_OPEN.store(false, Ordering::Release);
        apply_close_behavior(hwnd, behavior);
    } else if show_close_confirmation(hwnd).is_err() {
        CLOSE_DIALOG_OPEN.store(false, Ordering::Release);
    }
}

fn apply_close_behavior(hwnd: HWND, behavior: CloseBehavior) {
    match behavior {
        CloseBehavior::Exit => exit_application(),
        CloseBehavior::Close => {
            let _ = post_allowed_close(hwnd);
        }
    }
}

fn post_allowed_close(hwnd: HWND) -> bool {
    let bypass = CLOSE_SUBCLASS_INSTALLED.load(Ordering::Acquire);
    if bypass {
        ALLOW_CLOSE.store(true, Ordering::Release);
    }

    let posted =
        unsafe { PostMessageW(Some(hwnd), WM_CLOSE as u32, WPARAM(0), LPARAM(0)).as_bool() };
    if bypass && !posted {
        ALLOW_CLOSE.store(false, Ordering::Release);
    }
    posted
}

fn show_close_confirmation(hwnd: HWND) -> windows::core::Result<()> {
    let xaml_root = with_active_host(|host| -> windows::core::Result<IUnknown> {
        let window = host.window().cast::<IWindow>()?;
        let content = window.content()?;
        content.cast::<IUIElement>()?.xaml_root()
    })
    .ok_or_else(windows::core::Error::empty)??;

    let dialog = ContentDialog::new()?;
    let checkbox = CheckBox::new()?;
    let remember_text = windows_reference::IReference::<HSTRING>::from("记住这个选择");
    checkbox
        .cast::<IContentControl>()?
        .set_content(&remember_text)?;

    let dialog_content = dialog.cast::<IContentControl>()?;
    dialog_content.set_content(&checkbox)?;
    dialog.cast::<IUIElement>()?.set_xaml_root(&xaml_root)?;

    let dialog = dialog.cast::<IContentDialog>()?;
    let title = windows_reference::IReference::<HSTRING>::from("关闭 KumoRust");
    dialog.set_title(&title)?;
    dialog.set_primary_button_text("退出程序")?;
    dialog.set_secondary_button_text("仅关闭窗口")?;
    dialog.set_close_button_text("取消")?;

    let checkbox_for_handler = checkbox.clone();
    let hwnd_raw = hwnd.0 as isize;
    dialog.closed(move |_sender, args| {
        let result = args
            .as_ref()
            .and_then(|args| args.cast::<IContentDialogClosedEventArgs>().ok())
            .and_then(|args| args.result().ok())
            .unwrap_or_default();
        let remember = checkbox_for_handler
            .cast::<IToggleButton>()
            .map(|checkbox| checkbox.is_checked())
            .unwrap_or(false);

        CLOSE_DIALOG.with(|slot| {
            let _ = slot.borrow_mut().take();
        });
        CLOSE_DIALOG_OPEN.store(false, Ordering::Release);

        let behavior = match result {
            1 => Some(CloseBehavior::Exit),
            2 => Some(CloseBehavior::Close),
            _ => None,
        };
        if let Some(behavior) = behavior {
            if remember {
                let _ = settings::save_close_behavior(behavior);
            }
            apply_close_behavior(HWND(hwnd_raw as *mut c_void), behavior);
        }
        Ok(())
    })?;

    CLOSE_DIALOG.with(|slot| {
        *slot.borrow_mut() = Some(CloseDialogState {
            _dialog: dialog.clone().cast().unwrap_or_else(|_| unreachable!()),
            _checkbox: checkbox.clone(),
            _operation: None,
        });
    });

    let operation = match dialog.show_async() {
        Ok(operation) => operation,
        Err(error) => {
            CLOSE_DIALOG.with(|slot| {
                let _ = slot.borrow_mut().take();
            });
            return Err(error);
        }
    };
    CLOSE_DIALOG.with(|slot| {
        if let Some(state) = slot.borrow_mut().as_mut() {
            state._operation = Some(operation);
        }
    });
    Ok(())
}

fn hide_titlebar_icon(window: &impl Interface) {
    let result = (|| -> windows::core::Result<()> {
        let window = window.cast::<IWindow2>()?;
        let app_window = window.app_window()?.cast::<AppWindow>()?;
        let title_bar = app_window.title_bar()?.cast::<AppWindowTitleBar>()?;
        unsafe { (title_bar.vtable().set_icon_show_options)(title_bar.as_raw(), 1).ok()? }
        Ok(())
    })();

    let _ = result;
}

