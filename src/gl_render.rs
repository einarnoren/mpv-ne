//! Phase 1 of the GPU zero-copy rendering effort: get mpv rendering via its
//! OpenGL embedder API instead of the `sw` (software) one, into an FBO we
//! own. This phase still reads the FBO back to a CPU buffer at the end and
//! feeds it into the exact same downstream pipeline as the software
//! renderer (`PlayerEvent::Frame`) - the point isn't a performance win yet,
//! it's proving the OpenGL context/FBO/render-call chain is solid on real
//! hardware before attempting the actually-risky part (sharing that FBO's
//! texture into wgpu's Vulkan/DX12 backend without a CPU round-trip, via
//! GPU vendor interop extensions).
//!
//! mpv's embeddable render API (libmpv/render.h) only ever supports two
//! backends: `MPV_RENDER_API_TYPE_SW` and `MPV_RENDER_API_TYPE_OPENGL` -
//! there is no D3D11 or Vulkan option for embedders (confirmed against
//! mpv's own upstream headers; a 2018 feature request for D3D11 embedder
//! support was never implemented). OpenGL is therefore the only path to a
//! GPU-resident frame at all via libmpv on any platform.
//!
//! No `windows`/`gl` crate dependency - hand-written WGL/GL FFI, matching
//! the rest of this codebase's Win32 interop style (see win32_modal.rs).
#![cfg(target_os = "windows")]

use std::ffi::{CString, c_void};
use std::sync::atomic::{AtomicBool, Ordering};

use libmpv_sys as sys;

use crate::player::PlayerEvent;

type HWND = isize;
type HDC = isize;
type HGLRC = isize;

#[repr(C)]
struct PixelFormatDescriptor {
    n_size: u16,
    n_version: u16,
    dw_flags: u32,
    i_pixel_type: u8,
    c_color_bits: u8,
    c_red_bits: u8,
    c_red_shift: u8,
    c_green_bits: u8,
    c_green_shift: u8,
    c_blue_bits: u8,
    c_blue_shift: u8,
    c_alpha_bits: u8,
    c_alpha_shift: u8,
    c_accum_bits: u8,
    c_accum_red_bits: u8,
    c_accum_green_bits: u8,
    c_accum_blue_bits: u8,
    c_accum_alpha_bits: u8,
    c_depth_bits: u8,
    c_stencil_bits: u8,
    c_aux_buffers: u8,
    i_layer_type: u8,
    b_reserved: u8,
    dw_layer_mask: u32,
    dw_visible_mask: u32,
    dw_damage_mask: u32,
}

const PFD_DRAW_TO_WINDOW: u32 = 0x4;
const PFD_SUPPORT_OPENGL: u32 = 0x20;
const PFD_DOUBLEBUFFER: u32 = 0x1;
const PFD_TYPE_RGBA: u8 = 0;
const WS_POPUP: u32 = 0x80000000u32;

#[link(name = "user32")]
unsafe extern "system" {
    fn RegisterClassW(class: *const WndClassW) -> u16;
    fn CreateWindowExW(
        ex_style: u32, class_name: *const u16, window_name: *const u16, style: u32,
        x: i32, y: i32, w: i32, h: i32, parent: HWND, menu: isize, instance: isize,
        param: *mut c_void,
    ) -> HWND;
    fn DestroyWindow(hwnd: HWND) -> i32;
    fn GetDC(hwnd: HWND) -> HDC;
    fn ReleaseDC(hwnd: HWND, hdc: HDC) -> i32;
    fn DefWindowProcW(hwnd: HWND, msg: u32, w: usize, l: isize) -> isize;
    fn GetModuleHandleW(name: *const u16) -> isize;
}

#[repr(C)]
struct WndClassW {
    style: u32,
    lpfn_wnd_proc: unsafe extern "system" fn(HWND, u32, usize, isize) -> isize,
    cb_cls_extra: i32,
    cb_wnd_extra: i32,
    h_instance: isize,
    h_icon: isize,
    h_cursor: isize,
    hbr_background: isize,
    lpsz_menu_name: *const u16,
    lpsz_class_name: *const u16,
}

unsafe extern "system" fn dummy_wnd_proc(hwnd: HWND, msg: u32, w: usize, l: isize) -> isize {
    unsafe { DefWindowProcW(hwnd, msg, w, l) }
}

#[link(name = "gdi32")]
unsafe extern "system" {
    fn ChoosePixelFormat(hdc: HDC, pfd: *const PixelFormatDescriptor) -> i32;
    fn SetPixelFormat(hdc: HDC, format: i32, pfd: *const PixelFormatDescriptor) -> i32;
}

#[link(name = "opengl32")]
unsafe extern "system" {
    fn wglCreateContext(hdc: HDC) -> HGLRC;
    fn wglMakeCurrent(hdc: HDC, hglrc: HGLRC) -> i32;
    fn wglDeleteContext(hglrc: HGLRC) -> i32;
    fn wglGetProcAddress(name: *const i8) -> *mut c_void;
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetModuleHandleA(name: *const i8) -> isize;
    fn GetProcAddress(module: isize, name: *const i8) -> *mut c_void;
}

/// Resolve a GL function by name: try `wglGetProcAddress` first (the only
/// way to get anything beyond GL 1.1), falling back to the static export
/// table in opengl32.dll (which some drivers don't return via
/// wglGetProcAddress even for functions they do export - mostly matters
/// for old GL1.1 entry points like glViewport/glReadPixels/glGetError,
/// which mpv itself may ask for through `mpv_get_proc_address`).
fn resolve_gl_proc(name: &std::ffi::CStr) -> *mut c_void {
    let p = unsafe { wglGetProcAddress(name.as_ptr()) };
    // Drivers signal "not found" with NULL or one of a few small sentinel
    // values (1/2/3/-1), per the documented wglGetProcAddress contract.
    if !p.is_null() && !(1..=3).contains(&(p as isize)) && (p as isize) != -1 {
        return p;
    }
    let module_name = c"opengl32.dll";
    let module = unsafe { GetModuleHandleA(module_name.as_ptr()) };
    if module == 0 {
        return std::ptr::null_mut();
    }
    unsafe { GetProcAddress(module, name.as_ptr()) }
}

// A handful of GL 1.1 functions statically exported by opengl32.dll - safe
// to link directly. Everything past GL 1.1 (framebuffers, etc.) has to be
// loaded dynamically via `wglGetProcAddress` instead - see `GlFns`.
#[link(name = "opengl32")]
unsafe extern "system" {
    fn glGetError() -> u32;
    fn glReadPixels(x: i32, y: i32, w: i32, h: i32, format: u32, ty: u32, pixels: *mut c_void);
}

const GL_RGBA: u32 = 0x1908;
const GL_UNSIGNED_BYTE: u32 = 0x1401;
const GL_FRAMEBUFFER: u32 = 0x8D40;
const GL_COLOR_ATTACHMENT0: u32 = 0x8CE0;
const GL_TEXTURE_2D: u32 = 0x0DE1;
const GL_RGBA8: u32 = 0x8058;
const GL_FRAMEBUFFER_COMPLETE: u32 = 0x8CD5;
const GL_LINEAR: u32 = 0x2601;
const GL_TEXTURE_MIN_FILTER: u32 = 0x2801;
const GL_TEXTURE_MAG_FILTER: u32 = 0x2800;

type GlGenFramebuffers = unsafe extern "system" fn(n: i32, ids: *mut u32);
type GlBindFramebuffer = unsafe extern "system" fn(target: u32, fb: u32);
type GlDeleteFramebuffers = unsafe extern "system" fn(n: i32, ids: *const u32);
type GlFramebufferTexture2D = unsafe extern "system" fn(target: u32, attachment: u32, textarget: u32, tex: u32, level: i32);
type GlCheckFramebufferStatus = unsafe extern "system" fn(target: u32) -> u32;
type GlGenTextures = unsafe extern "system" fn(n: i32, ids: *mut u32);
type GlDeleteTextures = unsafe extern "system" fn(n: i32, ids: *const u32);
type GlBindTexture = unsafe extern "system" fn(target: u32, tex: u32);
type GlTexImage2D = unsafe extern "system" fn(target: u32, level: i32, internalformat: i32, w: i32, h: i32, border: i32, format: u32, ty: u32, pixels: *const c_void);
type GlTexParameteri = unsafe extern "system" fn(target: u32, pname: u32, param: i32);

/// GL >1.1 functions, loaded dynamically once a context is current. mpv
/// itself loads whatever *it* needs the same way (see `get_proc_address`
/// below) - this set is just what our own FBO/readback bookkeeping needs.
struct GlFns {
    gen_framebuffers: GlGenFramebuffers,
    bind_framebuffer: GlBindFramebuffer,
    delete_framebuffers: GlDeleteFramebuffers,
    framebuffer_texture_2d: GlFramebufferTexture2D,
    check_framebuffer_status: GlCheckFramebufferStatus,
    gen_textures: GlGenTextures,
    delete_textures: GlDeleteTextures,
    bind_texture: GlBindTexture,
    tex_image_2d: GlTexImage2D,
    tex_parameteri: GlTexParameteri,
}

fn gl_get_proc(name: &str) -> *mut c_void {
    let c = CString::new(name).unwrap();
    resolve_gl_proc(&c)
}

impl GlFns {
    unsafe fn load() -> Option<Self> {
        macro_rules! load {
            ($name:literal) => {{
                let p = gl_get_proc($name);
                if p.is_null() {
                    tracing::error!(name = $name, "gl_render: failed to load GL function");
                    return None;
                }
                unsafe { std::mem::transmute(p) }
            }};
        }
        Some(Self {
            gen_framebuffers: load!("glGenFramebuffers"),
            bind_framebuffer: load!("glBindFramebuffer"),
            delete_framebuffers: load!("glDeleteFramebuffers"),
            framebuffer_texture_2d: load!("glFramebufferTexture2D"),
            check_framebuffer_status: load!("glCheckFramebufferStatus"),
            gen_textures: load!("glGenTextures"),
            delete_textures: load!("glDeleteTextures"),
            bind_texture: load!("glBindTexture"),
            tex_image_2d: load!("glTexImage2D"),
            tex_parameteri: load!("glTexParameteri"),
        })
    }
}

/// Owns the hidden window + WGL context mpv renders into. Never shown,
/// never receives real messages - it exists purely to give WGL a valid
/// HDC/pixel-format to create a context against, which OpenGL requires.
struct GlContext {
    hwnd: HWND,
    hdc: HDC,
    hglrc: HGLRC,
    fns: GlFns,
}

impl GlContext {
    fn create() -> Option<Self> {
        unsafe {
            let instance = GetModuleHandleW(std::ptr::null());
            let class_name: Vec<u16> = "MPVNE_GL_HIDDEN\0".encode_utf16().collect();
            let class = WndClassW {
                style: 0,
                lpfn_wnd_proc: dummy_wnd_proc,
                cb_cls_extra: 0,
                cb_wnd_extra: 0,
                h_instance: instance,
                h_icon: 0,
                h_cursor: 0,
                hbr_background: 0,
                lpsz_menu_name: std::ptr::null(),
                lpsz_class_name: class_name.as_ptr(),
            };
            // Ignore failure - a second call (e.g. after a restart) will
            // fail with "class already exists", which is fine.
            RegisterClassW(&class);

            let hwnd = CreateWindowExW(
                0, class_name.as_ptr(), class_name.as_ptr(), WS_POPUP,
                0, 0, 64, 64, 0, 0, instance, std::ptr::null_mut(),
            );
            if hwnd == 0 {
                tracing::error!("gl_render: hidden window creation failed");
                return None;
            }

            let hdc = GetDC(hwnd);
            if hdc == 0 {
                DestroyWindow(hwnd);
                return None;
            }

            let pfd = PixelFormatDescriptor {
                n_size: std::mem::size_of::<PixelFormatDescriptor>() as u16,
                n_version: 1,
                dw_flags: PFD_DRAW_TO_WINDOW | PFD_SUPPORT_OPENGL | PFD_DOUBLEBUFFER,
                i_pixel_type: PFD_TYPE_RGBA,
                c_color_bits: 32,
                c_red_bits: 0, c_red_shift: 0, c_green_bits: 0, c_green_shift: 0,
                c_blue_bits: 0, c_blue_shift: 0, c_alpha_bits: 0, c_alpha_shift: 0,
                c_accum_bits: 0, c_accum_red_bits: 0, c_accum_green_bits: 0,
                c_accum_blue_bits: 0, c_accum_alpha_bits: 0,
                c_depth_bits: 24, c_stencil_bits: 8, c_aux_buffers: 0,
                i_layer_type: 0, b_reserved: 0, dw_layer_mask: 0, dw_visible_mask: 0, dw_damage_mask: 0,
            };
            let fmt = ChoosePixelFormat(hdc, &pfd);
            if fmt == 0 || SetPixelFormat(hdc, fmt, &pfd) == 0 {
                tracing::error!("gl_render: pixel format setup failed");
                ReleaseDC(hwnd, hdc);
                DestroyWindow(hwnd);
                return None;
            }

            let hglrc = wglCreateContext(hdc);
            if hglrc == 0 {
                tracing::error!("gl_render: wglCreateContext failed");
                ReleaseDC(hwnd, hdc);
                DestroyWindow(hwnd);
                return None;
            }
            if wglMakeCurrent(hdc, hglrc) == 0 {
                tracing::error!("gl_render: wglMakeCurrent failed");
                wglDeleteContext(hglrc);
                ReleaseDC(hwnd, hdc);
                DestroyWindow(hwnd);
                return None;
            }

            let Some(fns) = GlFns::load() else {
                wglMakeCurrent(0, 0);
                wglDeleteContext(hglrc);
                ReleaseDC(hwnd, hdc);
                DestroyWindow(hwnd);
                return None;
            };

            tracing::info!("gl_render: WGL context created and current");
            Some(Self { hwnd, hdc, hglrc, fns })
        }
    }

    fn make_current(&self) -> bool {
        unsafe { wglMakeCurrent(self.hdc, self.hglrc) != 0 }
    }
}

impl Drop for GlContext {
    fn drop(&mut self) {
        unsafe {
            wglMakeCurrent(0, 0);
            wglDeleteContext(self.hglrc);
            ReleaseDC(self.hwnd, self.hdc);
            DestroyWindow(self.hwnd);
        }
    }
}

/// mpv calls this itself (via a raw function pointer, no closure capture -
/// see `mpv_opengl_init_params`) whenever it needs to resolve a GL function
/// by name for its own internal use. Must be `extern "C"` to match libmpv's
/// declared callback signature (a plain C library, not a Win32 API - unlike
/// most of this codebase's other Windows FFI, which is `extern "system"`).
unsafe extern "C" fn mpv_get_proc_address(_ctx: *mut c_void, name: *const std::os::raw::c_char) -> *mut c_void {
    let name = unsafe { std::ffi::CStr::from_ptr(name) };
    resolve_gl_proc(name)
}

/// mpv's render-context update callback - same logic as
/// `player::wakeup_cb`, duplicated locally since that one is private to
/// `player.rs`. Just flags the render loop's wait condition and wakes it.
unsafe extern "C" fn gl_wakeup_cb(ctx: *mut c_void) {
    let arc = unsafe { &*(ctx as *const std::sync::Arc<crate::player::RenderSize>) };
    *arc.wake_flag.lock().unwrap() = true;
    arc.wake_cv.notify_one();
}

static GL_RENDER_FAILED: AtomicBool = AtomicBool::new(false);

/// Whether the user has opted into the OpenGL render path (persisted
/// setting, read once when each stream's render loop is spawned). Set at
/// boot from `InterfaceSettings::gl_render`. Also force-enabled by the
/// `MPVNE_GL_RENDER=1` env var for development/testing regardless of the
/// setting.
static GL_RENDER_ENABLED: AtomicBool = AtomicBool::new(false);

pub fn set_gl_render_enabled(on: bool) {
    GL_RENDER_ENABLED.store(on, Ordering::Relaxed);
}

/// True if the OpenGL render path should be used for newly-opened streams:
/// the setting is on (or the dev env var forces it) AND it hasn't already
/// failed to initialize earlier this run.
pub fn gl_render_wanted() -> bool {
    let enabled = GL_RENDER_ENABLED.load(Ordering::Relaxed)
        || std::env::var("MPVNE_GL_RENDER").is_ok_and(|v| v == "1");
    enabled && !gl_render_unavailable()
}

/// True if a previous attempt at the OpenGL render path failed (context
/// creation, FBO setup, or a render call) - callers should fall back to the
/// software renderer instead of retrying a path already known to be broken
/// on this machine/driver.
pub fn gl_render_unavailable() -> bool {
    GL_RENDER_FAILED.load(Ordering::Relaxed)
}

/// Render loop mirroring `player::init_and_render_loop`'s structure and
/// wake/throttle logic exactly, but using mpv's OpenGL embedder API into an
/// FBO instead of the `sw` API into a plain buffer. Phase 1 only: still
/// reads the FBO back to a CPU buffer via `glReadPixels` and forwards it
/// through the same `PlayerEvent::Frame` path the software renderer uses -
/// see the module doc comment for why.
///
/// Returns `true` if the OpenGL path initialized successfully and ran (the
/// caller should NOT start the software loop). Returns `false` if it failed
/// to even initialize (context/render-context creation) - the caller should
/// fall back to the software renderer so playback isn't left with no render
/// loop running at all. Mid-loop failures set the `gl_render_unavailable`
/// flag (so the *next* file uses software) but still return `true`, since
/// the mpv render context already exists and a clean handoff mid-stream
/// isn't worth the added complexity for that rare case.
pub fn init_and_render_loop_gl(
    handle: std::sync::Arc<crate::player::Handle>,
    render_size: std::sync::Arc<crate::player::RenderSize>,
    tx: tokio::sync::mpsc::UnboundedSender<PlayerEvent>,
) -> bool {
    let Some(gl) = GlContext::create() else {
        GL_RENDER_FAILED.store(true, Ordering::Relaxed);
        return false;
    };

    let mut init_params = sys::mpv_opengl_init_params {
        get_proc_address: Some(mpv_get_proc_address),
        get_proc_address_ctx: std::ptr::null_mut(),
        extra_exts: std::ptr::null(),
    };
    let api = sys::MPV_RENDER_API_TYPE_OPENGL.as_ptr() as *mut std::os::raw::c_void;
    let mut params = [
        sys::mpv_render_param { type_: sys::mpv_render_param_type_MPV_RENDER_PARAM_API_TYPE, data: api },
        sys::mpv_render_param {
            type_: sys::mpv_render_param_type_MPV_RENDER_PARAM_OPENGL_INIT_PARAMS,
            data: &mut init_params as *mut _ as *mut std::os::raw::c_void,
        },
        sys::mpv_render_param { type_: sys::mpv_render_param_type_MPV_RENDER_PARAM_INVALID, data: std::ptr::null_mut() },
    ];

    let mut ctx: *mut sys::mpv_render_context = std::ptr::null_mut();
    let rc = unsafe { sys::mpv_render_context_create(&mut ctx, handle.raw(), params.as_mut_ptr()) };
    if rc != 0 {
        tracing::error!(rc, "gl_render: mpv_render_context_create (opengl) failed");
        GL_RENDER_FAILED.store(true, Ordering::Relaxed);
        return false;
    }
    tracing::info!("gl_render: OpenGL render context ready");

    let cb_box = Box::new(std::sync::Arc::clone(&render_size));
    let cb_ptr = Box::into_raw(cb_box) as *mut std::ffi::c_void;
    unsafe {
        sys::mpv_render_context_set_update_callback(ctx, Some(gl_wakeup_cb), cb_ptr);
    }

    let fns = &gl.fns;
    let mut fbo: u32 = 0;
    let mut tex: u32 = 0;
    let mut last_w: u32 = 0;
    let mut last_h: u32 = 0;
    let mut last_frame_sent = std::time::Instant::now()
        .checked_sub(std::time::Duration::from_secs(1))
        .unwrap_or_else(std::time::Instant::now);
    const FRAME_THROTTLE: std::time::Duration = std::time::Duration::from_millis(33);

    loop {
        {
            let mut flag = render_size.wake_flag.lock().unwrap();
            while !*flag {
                flag = render_size.wake_cv.wait(flag).unwrap();
            }
            *flag = false;
        }

        let w = render_size.w.load(std::sync::atomic::Ordering::Relaxed);
        let h = render_size.h.load(std::sync::atomic::Ordering::Relaxed);
        if w == 0 || h == 0 {
            continue;
        }

        if !gl.make_current() {
            tracing::error!("gl_render: wglMakeCurrent failed mid-loop");
            GL_RENDER_FAILED.store(true, Ordering::Relaxed);
            break;
        }

        let size_changed = (w, h) != (last_w, last_h);
        let flags = unsafe { sys::mpv_render_context_update(ctx) };
        let frame_ready = flags & 1 != 0;
        if !frame_ready && !size_changed {
            continue;
        }
        last_w = w;
        last_h = h;

        if size_changed || fbo == 0 {
            unsafe {
                if fbo != 0 {
                    (fns.delete_framebuffers)(1, &fbo);
                    (fns.delete_textures)(1, &tex);
                }
                (fns.gen_textures)(1, &mut tex);
                (fns.bind_texture)(GL_TEXTURE_2D, tex);
                (fns.tex_parameteri)(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_LINEAR as i32);
                (fns.tex_parameteri)(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_LINEAR as i32);
                (fns.tex_image_2d)(GL_TEXTURE_2D, 0, GL_RGBA8 as i32, w as i32, h as i32, 0, GL_RGBA, GL_UNSIGNED_BYTE, std::ptr::null());
                (fns.gen_framebuffers)(1, &mut fbo);
                (fns.bind_framebuffer)(GL_FRAMEBUFFER, fbo);
                (fns.framebuffer_texture_2d)(GL_FRAMEBUFFER, GL_COLOR_ATTACHMENT0, GL_TEXTURE_2D, tex, 0);
                let status = (fns.check_framebuffer_status)(GL_FRAMEBUFFER);
                if status != GL_FRAMEBUFFER_COMPLETE {
                    tracing::error!(status, "gl_render: FBO incomplete");
                    GL_RENDER_FAILED.store(true, Ordering::Relaxed);
                    break;
                }
            }
        }

        let mut fbo_param = sys::mpv_opengl_fbo {
            fbo: fbo as i32,
            w: w as i32,
            h: h as i32,
            internal_format: GL_RGBA8 as i32,
        };
        // Empirically determined against this pipeline's actual glReadPixels
        // + downstream RGBA-buffer convention (row 0 = top) - the opposite
        // value produced upside-down output.
        let mut flip: std::os::raw::c_int = 0;
        let mut rp = [
            sys::mpv_render_param {
                type_: sys::mpv_render_param_type_MPV_RENDER_PARAM_OPENGL_FBO,
                data: &mut fbo_param as *mut _ as *mut std::os::raw::c_void,
            },
            sys::mpv_render_param {
                type_: sys::mpv_render_param_type_MPV_RENDER_PARAM_FLIP_Y,
                data: &mut flip as *mut _ as *mut std::os::raw::c_void,
            },
            sys::mpv_render_param { type_: sys::mpv_render_param_type_MPV_RENDER_PARAM_INVALID, data: std::ptr::null_mut() },
        ];
        let rc = unsafe { sys::mpv_render_context_render(ctx, rp.as_mut_ptr()) };
        if rc != 0 {
            tracing::warn!(rc, "gl_render: mpv_render_context_render failed");
            continue;
        }

        let now = std::time::Instant::now();
        let should_send = size_changed || now.duration_since(last_frame_sent) >= FRAME_THROTTLE;
        if !should_send {
            continue;
        }
        last_frame_sent = now;

        let mut buf = vec![0u8; (w * h * 4) as usize];
        unsafe {
            (fns.bind_framebuffer)(GL_FRAMEBUFFER, fbo);
            glReadPixels(0, 0, w as i32, h as i32, GL_RGBA, GL_UNSIGNED_BYTE, buf.as_mut_ptr() as *mut c_void);
            let err = glGetError();
            if err != 0 {
                tracing::warn!(err, "gl_render: glReadPixels reported a GL error");
            }
        }
        if tx.send(PlayerEvent::Frame(buf, w, h)).is_err() {
            break;
        }
    }

    unsafe {
        sys::mpv_render_context_set_update_callback(ctx, None, std::ptr::null_mut());
        sys::mpv_render_context_free(ctx);
        if fbo != 0 {
            (fns.delete_framebuffers)(1, &fbo);
            (fns.delete_textures)(1, &tex);
        }
    }
    // Reached only after successful init - the render context existed and
    // the loop exited normally (channel closed) or on a rare mid-loop GL
    // error. Either way, don't have the caller start a second (software)
    // loop; the failure flag already routes the next file to software.
    true
}
