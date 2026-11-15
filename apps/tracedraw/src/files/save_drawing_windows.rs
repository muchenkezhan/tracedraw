//! The Windows Save Drawing window, with format options in the system dialog.

mod rating;

use crate::i18n::{tr, trf};
use crate::ui::save_as::{
    fonts_left_out_text, size_text, tdraw_version_choices, tdraw_version_label, version_choices,
    version_label, SaveAsState,
};
use std::cell::{Cell, RefCell};
use std::ffi::OsString;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Component, Path, PathBuf, Prefix};
use std::rc::Rc;
use windows::core::{implement, Error, Interface, Ref, Result, HSTRING, PWSTR};
use windows::Win32::Foundation::{ERROR_CANCELLED, E_FAIL, HWND, RECT};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_INPROC_SERVER,
    COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
};
use windows::Win32::System::Ole::IOleWindow;
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::Win32::UI::Shell::{
    FileSaveDialog, IFileDialog, IFileDialogControlEvents, IFileDialogControlEvents_Impl,
    IFileDialogCustomize, IFileDialogEvents, IFileDialogEvents_Impl, IFileSaveDialog, IShellItem,
    SHCreateItemFromParsingName, CDCONTROLSTATEF, CDCS_ENABLEDVISIBLE, CDCS_INACTIVE, CDCS_VISIBLE,
    FDAP_BOTTOM, FDEOR_DEFAULT, FDESVR_DEFAULT, FDE_OVERWRITE_RESPONSE,
    FDE_SHAREVIOLATION_RESPONSE, FOS_FORCEFILESYSTEM, FOS_NOCHANGEDIR, FOS_OVERWRITEPROMPT,
    FOS_PATHMUSTEXIST, FOS_STRICTFILETYPES, SIGDN_FILESYSPATH,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindow, GetWindowRect, KillTimer, SetTimer, SetWindowPos, GW_OWNER, SWP_NOACTIVATE,
    SWP_NOSIZE, SWP_NOZORDER, USER_TIMER_MINIMUM,
};

const TITLE: u32 = 101;
const SUBJECT: u32 = 102;
const CDR_VERSION: u32 = 104;
const TDRAW_VERSION: u32 = 105;
const SELECTED: u32 = 106;
const PROFILE: u32 = 107;
const CDR_FONTS: u32 = 108;
const TDRAW_FONTS: u32 = 109;
const PACKAGE: u32 = 110;
const PACKAGE_PDF: u32 = 111;
const PROFILE_NOTE: u32 = 112;
const FONTS_NOTE: u32 = 113;
const CDR_VERSION_GROUP: u32 = 204;
const TDRAW_VERSION_GROUP: u32 = 205;

/// Pair every successful apartment initialization, including S_FALSE, with its end.
struct Apartment(std::marker::PhantomData<Rc<()>>);

impl Apartment {
    fn enter() -> Result<Self> {
        // SAFETY: the dialog runs on the calling UI thread, in its STA apartment.
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE).ok()?;
        }
        Ok(Self(std::marker::PhantomData))
    }
}

impl Drop for Apartment {
    fn drop(&mut self) {
        // SAFETY: this guard is created only after a successful initialization.
        unsafe { CoUninitialize() };
    }
}

/// The shell allocates returned text with the COM task allocator.
struct ShellText(PWSTR);

impl ShellText {
    fn text(&self) -> String {
        if self.0.is_null() {
            return String::new();
        }
        // SAFETY: each instance owns the terminated UTF-16 buffer a shell method returned.
        unsafe { String::from_utf16_lossy(self.0.as_wide()) }
    }

    fn path(&self) -> PathBuf {
        if self.0.is_null() {
            return PathBuf::new();
        }
        // SAFETY: keep Windows paths lossless while the allocation remains owned here.
        unsafe { PathBuf::from(OsString::from_wide(self.0.as_wide())) }
    }
}

impl Drop for ShellText {
    fn drop(&mut self) {
        // SAFETY: the buffer came from a documented COM-allocated string result.
        unsafe { CoTaskMemFree(Some(self.0 .0.cast())) };
    }
}

/// Present the system window. Cancelling returns no state and changes no drawing.
pub(crate) fn show(
    state: SaveAsState,
    owner: isize,
) -> std::result::Result<Option<SaveAsState>, String> {
    let _apartment = Apartment::enter().map_err(|error| error.to_string())?;
    // All COM references in run are dropped before the apartment guard.
    run(state, owner).map_err(|error| error.to_string())
}

fn run(state: SaveAsState, owner: isize) -> Result<Option<SaveAsState>> {
    // SAFETY: the apartment is initialized for this thread and the interfaces stay on it.
    unsafe {
        let save: IFileSaveDialog = CoCreateInstance(&FileSaveDialog, None, CLSCTX_INPROC_SERVER)?;
        let dialog: IFileDialog = save.cast()?;
        let controls: IFileDialogCustomize = save.cast()?;
        dialog.SetOptions(
            dialog.GetOptions()?
                | FOS_FORCEFILESYSTEM
                | FOS_PATHMUSTEXIST
                | FOS_NOCHANGEDIR
                | FOS_OVERWRITEPROMPT
                | FOS_STRICTFILETYPES,
        )?;
        dialog.SetTitle(&HSTRING::from(tr("dialog.save_drawing")))?;
        dialog.SetOkButtonLabel(&HSTRING::from(tr("dialog.save")))?;
        dialog.SetFileNameLabel(&HSTRING::from(tr("dialog.file_name")))?;
        let native_label = HSTRING::from("TraceDraw (*.tdraw)");
        let cdr_label = HSTRING::from(tr("file.cdr_files"));
        dialog.SetFileTypes(&[
            COMDLG_FILTERSPEC {
                pszName: windows::core::PCWSTR(native_label.as_ptr()),
                pszSpec: windows::core::w!("*.tdraw"),
            },
            COMDLG_FILTERSPEC {
                pszName: windows::core::PCWSTR(cdr_label.as_ptr()),
                pszSpec: windows::core::w!("*.cdr"),
            },
        ])?;
        dialog.SetFileTypeIndex(if state.cdr { 2 } else { 1 })?;
        dialog.SetDefaultExtension(&HSTRING::from(state.extension()))?;
        dialog.SetFileName(&HSTRING::from(state.file_name()))?;
        if !state.dir.as_os_str().is_empty() {
            let folder: IShellItem = SHCreateItemFromParsingName(
                &HSTRING::from(shell_path(&state.dir).as_os_str()),
                None,
            )
            .inspect_err(|error| log::warn!("system save folder resolution: {error}"))?;
            dialog
                .SetFolder(&folder)
                .inspect_err(|error| log::warn!("system save folder selection: {error}"))?;
        }
        for path in &state.bookmarks {
            if !crate::files::is_dir(path) {
                continue;
            }
            let place: Result<IShellItem> =
                SHCreateItemFromParsingName(&HSTRING::from(shell_path(path).as_os_str()), None);
            match place.and_then(|place| dialog.AddPlace(&place, FDAP_BOTTOM)) {
                Ok(()) => {}
                Err(error) => log::warn!("could not show saved place {}: {error}", path.display()),
            }
        }
        rating::configure(&save, state.rating)?;
        add_controls(&controls, &state)
            .inspect_err(|error| log::warn!("system save control creation: {error}"))?;
        update_controls(&controls, &state)
            .inspect_err(|error| log::warn!("system save control state: {error}"))?;

        let state = Rc::new(RefCell::new(state));
        let error = Rc::new(Cell::new(None));
        let events: IFileDialogEvents = Events {
            state: state.clone(),
            error: error.clone(),
            updating: Cell::new(false),
            centering_scheduled: Cell::new(false),
        }
        .into();
        let cookie = dialog
            .Advise(&events)
            .inspect_err(|error| log::warn!("system save event connection: {error}"))?;
        let shown = dialog.Show(if owner == 0 {
            None
        } else {
            Some(HWND(owner as *mut _))
        });
        let unadvised = dialog.Unadvise(cookie);
        if let Some(error) = error.take() {
            log::warn!("system save event callback: {error}");
            return Err(error);
        }
        if let Err(error) = shown {
            if error.code() == windows::core::HRESULT::from_win32(ERROR_CANCELLED.0) {
                unadvised?;
                return Ok(None);
            }
            log::warn!("system save window: {error}");
            return Err(error);
        }
        unadvised?;
        let mut result = state.try_borrow().map_err(borrow_error)?.clone();
        // The shell retains its collected properties after closing, unlike
        // the custom edit controls captured by the FileOk notification.
        result.rating = rating::read(&save)?;
        let path = ShellText(dialog.GetResult()?.GetDisplayName(SIGDN_FILESYSPATH)?).path();
        let name = path
            .file_name()
            .ok_or_else(|| Error::from_hresult(E_FAIL))?;
        result.name = name.to_string_lossy().into_owned();
        result.dir = path
            .parent()
            .ok_or_else(|| Error::from_hresult(E_FAIL))?
            .to_path_buf();
        result.dir_text = result.dir.display().to_string();
        // The system has confirmed replacement of this path. A package's
        // nested drawing is still checked by the application's save flow.
        result.replace = Some(path);
        result.note = None;
        Ok(Some(result))
    }
}

/// The shell requires backslashes and ordinary absolute paths, while the
/// file APIs also accept slashes and verbatim paths returned by canonicalize.
/// Normalize only separators and prefixes, keeping Windows text lossless.
fn shell_path(path: &Path) -> PathBuf {
    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .map(|unit| {
            if unit == u16::from(b'/') {
                u16::from(b'\\')
            } else {
                unit
            }
        })
        .collect();
    let path = PathBuf::from(OsString::from_wide(&wide));
    let mut components = path.components();
    let Some(Component::Prefix(prefix)) = components.next() else {
        return path;
    };
    let mut result = match prefix.kind() {
        Prefix::VerbatimDisk(drive) => PathBuf::from(format!("{}:\\", char::from(drive))),
        Prefix::VerbatimUNC(server, share) => PathBuf::from("\\\\").join(server).join(share),
        _ => return path,
    };
    result.extend(components.filter(|part| !matches!(part, Component::RootDir)));
    result
}

fn borrow_error(error: impl std::fmt::Display) -> Error {
    Error::new(E_FAIL, error.to_string())
}

/// The initial folder notification precedes the shell's final layout. A
/// window-owned timer runs after that layout's higher-priority messages.
fn schedule_center(dialog: &IFileDialog) -> Result<()> {
    let window: IOleWindow = dialog.cast()?;
    // SAFETY: this HWND belongs to the save dialog on the calling thread.
    // The callback address gives its timer an identifier private to this module.
    unsafe {
        let hwnd = window.GetWindow()?;
        let id = center_after_layout as *const () as usize;
        if SetTimer(
            Some(hwnd),
            id,
            USER_TIMER_MINIMUM,
            Some(center_after_layout),
        ) == 0
        {
            return Err(Error::from_thread());
        }
    }
    Ok(())
}

unsafe extern "system" fn center_after_layout(hwnd: HWND, _: u32, id: usize, _: u32) {
    // SAFETY: the system invokes this callback on the window's thread. Stop the
    // timer before moving the window so later navigation and dragging are free.
    if let Err(error) = unsafe { KillTimer(Some(hwnd), id) } {
        log::warn!("could not stop save dialog centering: {error}");
        return;
    }
    if let Err(error) = center_window(hwnd) {
        log::warn!("could not center the system save dialog: {error}");
    }
}

/// Center this save window on its owner within the monitor's work area.
fn center_window(hwnd: HWND) -> Result<()> {
    // SAFETY: called for the live save dialog from its own window timer.
    unsafe {
        let owner = GetWindow(hwnd, GW_OWNER).unwrap_or_default();
        let anchor = if owner.is_invalid() { hwnd } else { owner };
        let monitor = MonitorFromWindow(anchor, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        GetMonitorInfoW(monitor, &mut info).ok()?;
        let mut bounds = RECT::default();
        GetWindowRect(hwnd, &mut bounds)?;
        let mut parent = info.rcWork;
        if !owner.is_invalid() {
            GetWindowRect(owner, &mut parent)?;
        }
        let (x, y) = centered_position(bounds, parent, info.rcWork);
        SetWindowPos(
            hwnd,
            None,
            x,
            y,
            0,
            0,
            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        )
    }
}

fn centered_position(window: RECT, parent: RECT, work: RECT) -> (i32, i32) {
    let axis = |start: i32, end: i32, size: i64, low: i32, high: i32| {
        let low = i64::from(low);
        let high = (i64::from(high) - size).max(low);
        let center = i64::from(start) + (i64::from(end) - i64::from(start) - size) / 2;
        center.clamp(low, high) as i32
    };
    (
        axis(
            parent.left,
            parent.right,
            i64::from(window.right) - i64::from(window.left),
            work.left,
            work.right,
        ),
        axis(
            parent.top,
            parent.bottom,
            i64::from(window.bottom) - i64::from(window.top),
            work.top,
            work.bottom,
        ),
    )
}

fn add_controls(controls: &IFileDialogCustomize, state: &SaveAsState) -> Result<()> {
    // SAFETY: all strings live through the calls, which copy their labels and text.
    unsafe {
        for (group, control, key, text) in [
            (201, TITLE, "dialog.title", state.title.as_str()),
            (202, SUBJECT, "dialog.subject", state.subject.as_str()),
        ] {
            controls.StartVisualGroup(group, &HSTRING::from(tr(key)))?;
            controls.AddEditBox(control, &HSTRING::from(text))?;
            controls.EndVisualGroup()?;
        }
        controls.StartVisualGroup(CDR_VERSION_GROUP, &HSTRING::from(tr("dialog.cdr_version")))?;
        controls.AddComboBox(CDR_VERSION)?;
        let mut versions = version_choices();
        if !versions.contains(&state.version) {
            versions.push(state.version);
            versions.sort_unstable_by(|a, b| b.cmp(a));
        }
        for version in versions {
            controls.AddControlItem(
                CDR_VERSION,
                u32::from(version),
                &HSTRING::from(version_label(version)),
            )?;
        }
        controls.SetSelectedControlItem(CDR_VERSION, u32::from(state.version))?;
        controls.EndVisualGroup()?;
        controls.StartVisualGroup(
            TDRAW_VERSION_GROUP,
            &HSTRING::from(tr("dialog.cdr_version")),
        )?;
        controls.AddComboBox(TDRAW_VERSION)?;
        for version in tdraw_version_choices() {
            controls.AddControlItem(
                TDRAW_VERSION,
                version,
                &HSTRING::from(tdraw_version_label(version)),
            )?;
        }
        controls.SetSelectedControlItem(TDRAW_VERSION, state.tdraw_version)?;
        controls.EndVisualGroup()?;

        controls.AddCheckButton(
            SELECTED,
            &HSTRING::from(tr("dialog.save_selected_only")),
            state.selected_only,
        )?;
        let profile = state
            .profile
            .as_ref()
            .map(|(_, size)| {
                trf(
                    "dialog.embed_color_profile",
                    &[("size", &size_text(*size as u64))],
                )
            })
            .unwrap_or_else(|| tr("dialog.embed_color_profile_none"));
        controls.AddCheckButton(PROFILE, &HSTRING::from(profile), state.embed_profile)?;
        let cdr_fonts = if state.fonts_size > 0 {
            trf(
                "dialog.embed_fonts_size",
                &[("size", &size_text(state.fonts_size))],
            )
        } else {
            tr("dialog.embed_fonts")
        };
        controls.AddCheckButton(CDR_FONTS, &HSTRING::from(cdr_fonts), state.embed_fonts)?;
        let native_fonts = if state.tdraw_fonts_size > 0 {
            trf(
                "dialog.embed_fonts_size",
                &[("size", &size_text(state.tdraw_fonts_size))],
            )
        } else {
            tr("dialog.embed_fonts")
        };
        controls.AddCheckButton(
            TDRAW_FONTS,
            &HSTRING::from(native_fonts),
            state.tdraw_embed_fonts,
        )?;
        controls.AddCheckButton(
            PACKAGE,
            &HSTRING::from(tr("dialog.save_package")),
            state.package,
        )?;
        controls.AddCheckButton(
            PACKAGE_PDF,
            &HSTRING::from(tr("package.pdf")),
            state.package_pdf,
        )?;
        let profile_name = state
            .profile
            .as_ref()
            .map(|(name, _)| name.clone())
            .unwrap_or_else(|| tr("dialog.color_profile_missing"));
        controls.AddText(
            PROFILE_NOTE,
            &HSTRING::from(trf("dialog.color_profile", &[("name", &profile_name)])),
        )?;
        controls.AddText(
            FONTS_NOTE,
            &HSTRING::from(fonts_left_out_text(&state.fonts_left_out).unwrap_or_default()),
        )?;
    }
    Ok(())
}

fn control_state(visible: bool, enabled: bool) -> CDCONTROLSTATEF {
    if !visible {
        CDCS_INACTIVE
    } else if enabled {
        CDCS_ENABLEDVISIBLE
    } else {
        CDCS_VISIBLE
    }
}

fn update_controls(controls: &IFileDialogCustomize, state: &SaveAsState) -> Result<()> {
    let cdr_embeds = state.cdr && state.version >= 16;
    let native_fonts = !state.cdr && state.tdraw_fonts_size > 0;
    let native_versions = !state.cdr && tdraw_version_choices().len() > 1;
    // SAFETY: these IDs were added before showing the dialog. Only their
    // values, visibility and enabled state change while it is open.
    unsafe {
        for (id, visible, enabled) in [
            (CDR_VERSION_GROUP, state.cdr, true),
            (CDR_VERSION, state.cdr, true),
            (TDRAW_VERSION_GROUP, native_versions, true),
            (TDRAW_VERSION, native_versions, true),
            (SELECTED, true, state.has_selection),
            (PROFILE, state.cdr, cdr_embeds),
            (CDR_FONTS, state.cdr, cdr_embeds),
            (TDRAW_FONTS, !state.cdr, native_fonts),
            (PACKAGE, !state.cdr, !state.selected_only),
            (PACKAGE_PDF, !state.cdr, state.packaged()),
            (PROFILE_NOTE, state.cdr, true),
            (
                FONTS_NOTE,
                state.cdr && !state.fonts_left_out.is_empty(),
                true,
            ),
        ] {
            controls.SetControlState(id, control_state(visible, enabled))?;
        }
        controls.SetCheckButtonState(SELECTED, state.selected_only && state.has_selection)?;
        controls.SetCheckButtonState(PROFILE, state.embed_profile && cdr_embeds)?;
        controls.SetCheckButtonState(CDR_FONTS, state.embed_fonts && cdr_embeds)?;
        controls.SetCheckButtonState(TDRAW_FONTS, state.tdraw_embed_fonts && native_fonts)?;
        controls.SetCheckButtonState(PACKAGE, state.package)?;
        controls.SetCheckButtonState(PACKAGE_PDF, state.package_pdf)?;
    }
    Ok(())
}

fn read_controls(controls: &IFileDialogCustomize, state: &mut SaveAsState) -> Result<()> {
    // SAFETY: the dialog owns these controls until Show returns and every
    // string result is freed by ShellText, including conversion failures.
    unsafe {
        state.title = ShellText(PWSTR(controls.GetEditBoxText(TITLE)?)).text();
        state.subject = ShellText(PWSTR(controls.GetEditBoxText(SUBJECT)?)).text();
        state.selected_only =
            state.has_selection && controls.GetCheckButtonState(SELECTED)?.as_bool();
        if state.cdr {
            // Read the controls as they were displayed before a version
            // change, so an unavailable checkbox keeps the user's preference.
            if state.version >= 16 {
                state.embed_profile = controls.GetCheckButtonState(PROFILE)?.as_bool();
                state.embed_fonts = controls.GetCheckButtonState(CDR_FONTS)?.as_bool();
            }
            state.version = u16::try_from(controls.GetSelectedControlItem(CDR_VERSION)?)
                .map_err(borrow_error)?;
        } else {
            state.tdraw_version = controls.GetSelectedControlItem(TDRAW_VERSION)?;
            if state.tdraw_fonts_size > 0 {
                state.tdraw_embed_fonts = controls.GetCheckButtonState(TDRAW_FONTS)?.as_bool();
            }
            state.package = controls.GetCheckButtonState(PACKAGE)?.as_bool();
            state.package_pdf = controls.GetCheckButtonState(PACKAGE_PDF)?.as_bool();
        }
    }
    Ok(())
}

#[implement(IFileDialogEvents, IFileDialogControlEvents)]
struct Events {
    state: Rc<RefCell<SaveAsState>>,
    error: Rc<Cell<Option<Error>>>,
    updating: Cell<bool>,
    centering_scheduled: Cell<bool>,
}

enum Update {
    Controls,
    FileType,
    Accept,
}

struct Updating<'a>(&'a Cell<bool>);

impl Drop for Updating<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

impl Events {
    fn update(&self, dialog: &IFileDialog, kind: Update) -> Result<()> {
        if self.updating.replace(true) {
            return Ok(());
        }
        let _updating = Updating(&self.updating);
        let result = self.update_inner(dialog, kind);
        if let Err(error) = &result {
            // Windows may ignore an event callback's failure. Keep it for
            // the caller and end the modal loop instead of saving stale options.
            let first = self.error.take().unwrap_or_else(|| error.clone());
            self.error.set(Some(first));
            // SAFETY: the callback belongs to this live modal dialog.
            if let Err(close_error) = unsafe { dialog.Close(error.code()) } {
                log::warn!("could not close the failed save dialog: {close_error}");
            }
        }
        result
    }

    fn update_inner(&self, dialog: &IFileDialog, kind: Update) -> Result<()> {
        let mut state = self.state.try_borrow().map_err(borrow_error)?.clone();
        let controls: IFileDialogCustomize = dialog.cast()?;
        read_controls(&controls, &mut state)
            .inspect_err(|error| log::warn!("system save option collection: {error}"))?;
        let accepting = matches!(&kind, Update::Accept);
        match kind {
            Update::FileType => {
                // SAFETY: this callback follows the system's type selection.
                unsafe {
                    let cdr = dialog.GetFileTypeIndex()? == 2;
                    let name =
                        ShellText(dialog.GetFileName().inspect_err(|error| {
                            log::warn!("system save filename query: {error}")
                        })?)
                        .path();
                    if let Some(file_name) = name.file_name() {
                        state.name = file_name.to_string_lossy().into_owned();
                        state.set_cdr(cdr);
                        let file_name = name.with_file_name(state.file_name());
                        dialog.SetFileName(&HSTRING::from(file_name.as_os_str()))?;
                    } else {
                        state.set_cdr(cdr);
                    }
                    dialog.SetDefaultExtension(&HSTRING::from(state.extension()))?;
                }
            }
            Update::Accept | Update::Controls => {}
        }
        if !accepting {
            update_controls(&controls, &state)?;
        }
        *self.state.try_borrow_mut().map_err(borrow_error)? = state;
        Ok(())
    }
}

#[allow(non_snake_case)]
impl IFileDialogEvents_Impl for Events_Impl {
    fn OnFileOk(&self, dialog: Ref<IFileDialog>) -> Result<()> {
        self.update(dialog.ok()?, Update::Accept)
    }

    fn OnFolderChanging(&self, _: Ref<IFileDialog>, _: Ref<IShellItem>) -> Result<()> {
        Ok(())
    }
    fn OnFolderChange(&self, dialog: Ref<IFileDialog>) -> Result<()> {
        if !self.centering_scheduled.replace(true) {
            if let Err(error) = schedule_center(dialog.ok()?) {
                log::warn!("could not center the system save dialog: {error}");
            }
        }
        Ok(())
    }
    fn OnSelectionChange(&self, _: Ref<IFileDialog>) -> Result<()> {
        Ok(())
    }

    fn OnShareViolation(
        &self,
        _: Ref<IFileDialog>,
        _: Ref<IShellItem>,
    ) -> Result<FDE_SHAREVIOLATION_RESPONSE> {
        Ok(FDESVR_DEFAULT)
    }

    fn OnTypeChange(&self, dialog: Ref<IFileDialog>) -> Result<()> {
        self.update(dialog.ok()?, Update::FileType)
    }

    fn OnOverwrite(
        &self,
        _: Ref<IFileDialog>,
        _: Ref<IShellItem>,
    ) -> Result<FDE_OVERWRITE_RESPONSE> {
        Ok(FDEOR_DEFAULT)
    }
}

#[allow(non_snake_case)]
impl IFileDialogControlEvents_Impl for Events_Impl {
    fn OnItemSelected(&self, controls: Ref<IFileDialogCustomize>, _: u32, _: u32) -> Result<()> {
        self.update(&controls.ok()?.cast()?, Update::Controls)
    }

    fn OnCheckButtonToggled(
        &self,
        controls: Ref<IFileDialogCustomize>,
        _: u32,
        _: windows::core::BOOL,
    ) -> Result<()> {
        self.update(&controls.ok()?.cast()?, Update::Controls)
    }

    fn OnButtonClicked(&self, _: Ref<IFileDialogCustomize>, _: u32) -> Result<()> {
        Ok(())
    }
    fn OnControlActivating(&self, _: Ref<IFileDialogCustomize>, _: u32) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;

    #[test]
    fn native_save_centers_on_its_owner_within_the_monitor_work_area() {
        let window = RECT {
            left: 0,
            top: 0,
            right: 800,
            bottom: 600,
        };
        let work = RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1040,
        };
        let parent = RECT {
            left: 200,
            top: 100,
            right: 1800,
            bottom: 1000,
        };
        assert_eq!(centered_position(window, parent, work), (600, 250));
        let edge = RECT {
            left: 1600,
            top: 800,
            right: 2100,
            bottom: 1200,
        };
        assert_eq!(centered_position(window, edge, work), (1120, 440));
        let left_monitor = RECT {
            left: -1920,
            top: -100,
            right: 0,
            bottom: 940,
        };
        assert_eq!(
            centered_position(window, left_monitor, left_monitor),
            (-1360, 120)
        );
        let oversized = RECT {
            left: 0,
            top: 0,
            right: 2200,
            bottom: 1200,
        };
        assert_eq!(centered_position(oversized, parent, work), (0, 0));
    }

    #[test]
    fn native_save_shell_paths_keep_disk_unc_and_unicode_components() {
        for (source, expected) in [
            (r"\\?\D:\Drawings\海报", r"D:\Drawings\海报"),
            (r"\\?\UNC\server\share\海报", r"\\server\share\海报"),
            (r"D:\Drawings\海报", r"D:\Drawings\海报"),
            ("D:/Drawings/海报", r"D:\Drawings\海报"),
            (r"D:\Drawings/海报", r"D:\Drawings\海报"),
            ("//server/share/海报", r"\\server\share\海报"),
            (r"\\server\share/海报", r"\\server\share\海报"),
        ] {
            assert_eq!(
                shell_path(Path::new(source)).as_os_str(),
                Path::new(expected).as_os_str()
            );
        }
    }

    #[test]
    fn native_save_shell_paths_preserve_non_unicode_windows_text() {
        let mut source: Vec<u16> = r"\\?\D:\Drawings/海报".encode_utf16().collect();
        source.push(0xd800);
        let mut expected: Vec<u16> = r"D:\Drawings\海报".encode_utf16().collect();
        expected.push(0xd800);
        let path = PathBuf::from(OsString::from_wide(&source));
        assert_eq!(
            shell_path(&path)
                .as_os_str()
                .encode_wide()
                .collect::<Vec<_>>(),
            expected
        );
    }

    #[test]
    fn native_save_resolves_slashes_and_non_unicode_folders_with_the_shell() {
        let _apartment = Apartment::enter().expect("STA");
        let root =
            std::env::temp_dir().join(format!("tracedraw-shell-slashes-{}", std::process::id()));
        let mut name: Vec<u16> = "海报".encode_utf16().collect();
        name.push(0xd800);
        let folder = root.join(OsString::from_wide(&name));
        std::fs::create_dir_all(&folder).expect("test folder");
        let wide: Vec<u16> = folder.as_os_str().encode_wide().collect();
        let slashes: Vec<u16> = wide
            .iter()
            .map(|&unit| {
                if unit == u16::from(b'\\') {
                    u16::from(b'/')
                } else {
                    unit
                }
            })
            .collect();
        let mut mixed = wide.clone();
        let separator = mixed
            .iter()
            .rposition(|&unit| unit == u16::from(b'\\'))
            .expect("folder separator");
        mixed[separator] = u16::from(b'/');
        for source in [slashes, mixed] {
            let source = PathBuf::from(OsString::from_wide(&source));
            // SAFETY: the apartment and lossless path buffer live through the call.
            let item: IShellItem = unsafe {
                SHCreateItemFromParsingName(&HSTRING::from(shell_path(&source).as_os_str()), None)
                    .expect("the shell accepts a folder with normalized separators")
            };
            let selected =
                unsafe { ShellText(item.GetDisplayName(SIGDN_FILESYSPATH).expect("path")) }.path();
            assert_eq!(
                selected.canonicalize().expect("resolved folder"),
                folder.canonicalize().expect("created folder")
            );
        }
        std::fs::remove_dir(&folder).expect("remove test folder");
        std::fs::remove_dir(&root).expect("remove test root");
    }

    #[test]
    fn native_save_resolves_a_canonicalized_folder_with_the_shell() {
        let _apartment = Apartment::enter().expect("STA");
        let folder = std::env::temp_dir().canonicalize().expect("test folder");
        // SAFETY: the apartment and path buffer live through the call.
        let item: IShellItem = unsafe {
            SHCreateItemFromParsingName(&HSTRING::from(shell_path(&folder).as_os_str()), None)
                .expect("the shell accepts a canonicalized folder")
        };
        let selected =
            unsafe { ShellText(item.GetDisplayName(SIGDN_FILESYSPATH).expect("path")) }.path();
        assert_eq!(selected.canonicalize().expect("resolved folder"), folder);
    }

    fn make_dialog(
        state: SaveAsState,
    ) -> (
        IFileDialog,
        IFileDialogCustomize,
        IFileDialogEvents,
        Rc<RefCell<SaveAsState>>,
    ) {
        // SAFETY: each caller has initialized this test thread's apartment.
        unsafe {
            let save: IFileSaveDialog =
                CoCreateInstance(&FileSaveDialog, None, CLSCTX_INPROC_SERVER)
                    .expect("system dialog");
            let dialog: IFileDialog = save.cast().expect("file dialog");
            let controls: IFileDialogCustomize = save.cast().expect("customization");
            dialog
                .SetFileTypes(&[
                    COMDLG_FILTERSPEC {
                        pszName: windows::core::w!("TraceDraw"),
                        pszSpec: windows::core::w!("*.tdraw"),
                    },
                    COMDLG_FILTERSPEC {
                        pszName: windows::core::w!("CDR"),
                        pszSpec: windows::core::w!("*.cdr"),
                    },
                ])
                .expect("file types");
            dialog
                .SetFileTypeIndex(if state.cdr { 2 } else { 1 })
                .expect("type");
            dialog
                .SetFileName(&HSTRING::from(state.file_name()))
                .expect("name");
            rating::configure(&save, state.rating).expect("native rating property");
            add_controls(&controls, &state).expect("controls");
            update_controls(&controls, &state).expect("initial availability");
            let state = Rc::new(RefCell::new(state));
            let events: IFileDialogEvents = Events {
                state: state.clone(),
                error: Rc::new(Cell::new(None)),
                updating: Cell::new(false),
                centering_scheduled: Cell::new(false),
            }
            .into();
            (dialog, controls, events, state)
        }
    }

    #[test]
    fn native_save_captures_metadata_before_the_controls_are_released() {
        let _apartment = Apartment::enter().expect("STA");
        let mut initial = SaveAsState::new(&App::headless());
        initial.rating = 3;
        let (dialog, controls, _events, state) = make_dialog(initial);
        let controller = Events {
            state: state.clone(),
            error: Rc::new(Cell::new(None)),
            updating: Cell::new(false),
            centering_scheduled: Cell::new(false),
        };
        // SAFETY: the test owns this dialog and its controls on the current thread.
        unsafe {
            controls
                .SetEditBoxText(TITLE, windows::core::w!("Saved title"))
                .expect("title");
            controls
                .SetEditBoxText(SUBJECT, windows::core::w!("Saved subject"))
                .expect("subject");
            controller
                .update(&dialog, Update::Accept)
                .expect("accept snapshot");
            controls
                .SetEditBoxText(TITLE, windows::core::w!(""))
                .expect("released title");
            controls
                .SetEditBoxText(SUBJECT, windows::core::w!(""))
                .expect("released subject");
        }
        let accepted = state.borrow();
        assert_eq!(accepted.title, "Saved title");
        assert_eq!(accepted.subject, "Saved subject");
        assert_eq!(accepted.rating, 3);
    }

    #[test]
    fn native_save_options_keep_old_versions_and_embedding_preferences() {
        let _apartment = Apartment::enter().expect("STA");
        let mut initial = SaveAsState::new(&App::headless());
        initial.set_cdr(true);
        initial.version = 7;
        initial.embed_profile = true;
        initial.embed_fonts = true;
        let (dialog, controls, events, state) = make_dialog(initial);
        // SAFETY: calls use interfaces and controls created on this thread.
        unsafe {
            assert_eq!(
                controls
                    .GetSelectedControlItem(CDR_VERSION)
                    .expect("old version"),
                7
            );
            assert_eq!(
                controls.GetControlState(PROFILE).expect("disabled profile"),
                CDCS_VISIBLE
            );
            assert!(!controls
                .GetCheckButtonState(PROFILE)
                .expect("unchecked profile")
                .as_bool());
            controls
                .SetEditBoxText(TITLE, windows::core::w!("A new title"))
                .expect("title");
            dialog.SetFileTypeIndex(1).expect("native type");
            events.OnTypeChange(&dialog).expect("native options");
            assert!(!state.borrow().cdr);
            assert_eq!(state.borrow().title, "A new title");
            dialog.SetFileTypeIndex(2).expect("cdr type");
            events.OnTypeChange(&dialog).expect("cdr options");
            assert_eq!(state.borrow().version, 7);
            assert!(state.borrow().embed_profile && state.borrow().embed_fonts);
            controls
                .SetSelectedControlItem(CDR_VERSION, 16)
                .expect("new version");
            let control_events: IFileDialogControlEvents = events.cast().expect("control events");
            control_events
                .OnItemSelected(&controls, CDR_VERSION, 16)
                .expect("new availability");
            assert_eq!(
                controls.GetControlState(PROFILE).expect("profile"),
                CDCS_ENABLEDVISIBLE
            );
            assert!(controls
                .GetCheckButtonState(PROFILE)
                .expect("restored profile")
                .as_bool());
            assert!(controls
                .GetCheckButtonState(CDR_FONTS)
                .expect("restored fonts")
                .as_bool());
        }
    }

    #[test]
    fn native_save_selection_disables_package_without_losing_its_choice() {
        let _apartment = Apartment::enter().expect("STA");
        let mut initial = SaveAsState::new(&App::headless());
        initial.set_cdr(false);
        initial.has_selection = true;
        initial.package = true;
        initial.package_pdf = true;
        let (_, controls, events, state) = make_dialog(initial);
        // SAFETY: calls use interfaces and controls created on this thread.
        unsafe {
            let control_events: IFileDialogControlEvents = events.cast().expect("control events");
            controls
                .SetCheckButtonState(SELECTED, true)
                .expect("selected only");
            control_events
                .OnCheckButtonToggled(&controls, SELECTED, true)
                .expect("selection state");
            assert_eq!(
                controls.GetControlState(PACKAGE).expect("package"),
                CDCS_VISIBLE
            );
            assert_eq!(
                controls.GetControlState(PACKAGE_PDF).expect("pdf"),
                CDCS_VISIBLE
            );
            assert!(!state.borrow().packaged());
            assert!(state.borrow().package && state.borrow().package_pdf);
            controls
                .SetCheckButtonState(SELECTED, false)
                .expect("whole drawing");
            control_events
                .OnCheckButtonToggled(&controls, SELECTED, false)
                .expect("whole drawing state");
            assert_eq!(
                controls.GetControlState(PACKAGE).expect("package"),
                CDCS_ENABLEDVISIBLE
            );
            assert!(state.borrow().packaged());
            assert!(controls
                .GetCheckButtonState(PACKAGE_PDF)
                .expect("pdf remains checked")
                .as_bool());
        }
    }
}
