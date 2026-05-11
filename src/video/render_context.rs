use gl::types::*;
use libmpv2::Mpv;
use libmpv2_sys::{
    MPV_RENDER_API_TYPE_OPENGL, mpv_opengl_fbo, mpv_opengl_init_params, mpv_render_context,
    mpv_render_context_create, mpv_render_context_free, mpv_render_context_render,
    mpv_render_context_set_update_callback, mpv_render_context_update, mpv_render_param,
    mpv_render_param_type_MPV_RENDER_PARAM_API_TYPE as PARAM_API_TYPE,
    mpv_render_param_type_MPV_RENDER_PARAM_INVALID as PARAM_INVALID,
    mpv_render_param_type_MPV_RENDER_PARAM_OPENGL_FBO as PARAM_OPENGL_FBO,
    mpv_render_param_type_MPV_RENDER_PARAM_OPENGL_INIT_PARAMS as PARAM_OPENGL_INIT,
    mpv_render_update_flag_MPV_RENDER_UPDATE_FRAME as UPDATE_FRAME,
};
use std::ffi::{CStr, c_void};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum MpvError {
    RenderContextCreate(i32),
}

impl std::fmt::Display for MpvError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MpvError::RenderContextCreate(code) => {
                write!(f, "mpv_render_context_create failed (code {code})")
            }
        }
    }
}

impl std::error::Error for MpvError {}

// ---------------------------------------------------------------------------
// OpenGL framebuffer object helper
// ---------------------------------------------------------------------------

/// Owns a paired OpenGL FBO + backing texture and frees them on drop.
struct GlObjects {
    fbo: GLuint,
    tex: GLuint,
}

impl GlObjects {
    /// Allocate a new RGBA8 FBO of the given dimensions.
    ///
    /// # Safety
    /// An active OpenGL context must exist on the current thread.
    unsafe fn new(width: u32, height: u32) -> Self {
        let mut tex = 0;
        gl::GenTextures(1, &mut tex);
        gl::BindTexture(gl::TEXTURE_2D, tex);
        gl::TexImage2D(
            gl::TEXTURE_2D,
            0,
            gl::RGBA8 as GLint,
            width as GLsizei,
            height as GLsizei,
            0,
            gl::RGBA,
            gl::UNSIGNED_BYTE,
            std::ptr::null(),
        );
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR as GLint);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR as GLint);
        gl::BindTexture(gl::TEXTURE_2D, 0);

        let mut fbo = 0;
        gl::GenFramebuffers(1, &mut fbo);
        gl::BindFramebuffer(gl::FRAMEBUFFER, fbo);
        gl::FramebufferTexture2D(
            gl::FRAMEBUFFER,
            gl::COLOR_ATTACHMENT0,
            gl::TEXTURE_2D,
            tex,
            0,
        );
        gl::BindFramebuffer(gl::FRAMEBUFFER, 0);

        Self { fbo, tex }
    }
}

impl Drop for GlObjects {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteFramebuffers(1, &self.fbo);
            gl::DeleteTextures(1, &self.tex);
        }
    }
}

// ---------------------------------------------------------------------------
// Callbacks passed to mpv
// ---------------------------------------------------------------------------

/// Notifies the renderer that a new frame is ready.
unsafe extern "C" fn update_callback(cb_ctx: *mut c_void) {
    let flag = &*(cb_ctx as *const AtomicBool);
    flag.store(true, Ordering::Release);
}

/// Resolves an OpenGL symbol name using the caller-supplied loader.
///
/// `ctx` must point to a `&dyn Fn(&CStr) -> *const c_void` (a fat-pointer
/// reference stored behind a thin pointer via `&&dyn Fn`).
unsafe extern "C" fn get_proc_addr_mpv(
    ctx: *mut c_void,
    name: *const std::os::raw::c_char,
) -> *mut c_void {
    let loader = *(ctx as *const &dyn Fn(&CStr) -> *const c_void);
    loader(CStr::from_ptr(name)) as *mut c_void
}

// ---------------------------------------------------------------------------
// Public render context
// ---------------------------------------------------------------------------

pub struct GpuRenderContext {
    ctx: *mut mpv_render_context,
    pub width: u32,
    pub height: u32,
    gl: GlObjects,
    /// Exposes the backing texture so callers can composite the video frame.
    pub fbo_tex: GLuint,
    frame_ready: Arc<AtomicBool>,
}

impl GpuRenderContext {
    pub fn new(
        mpv: &Mpv,
        frame_ready: Arc<AtomicBool>,
        get_proc_address: &dyn Fn(&CStr) -> *const c_void,
        width: u32,
        height: u32,
    ) -> Result<Self, MpvError> {
        // Load OpenGL function pointers through the provided loader.
        gl::load_with(|s| {
            let c_str = std::ffi::CString::new(s).unwrap();
            get_proc_address(&c_str)
        });

        // Pass a `&&dyn Fn` so the thin pointer survives the FFI boundary while
        // preserving the fat pointer needed to call the trait object.
        let loader_ref = &get_proc_address;
        let loader_ctx = loader_ref as *const _ as *mut c_void;

        let mut gl_init = mpv_opengl_init_params {
            get_proc_address: Some(get_proc_addr_mpv),
            get_proc_address_ctx: loader_ctx,
        };

        let mut params = [
            mpv_render_param {
                type_: PARAM_API_TYPE,
                data: MPV_RENDER_API_TYPE_OPENGL.as_ptr() as *mut c_void,
            },
            mpv_render_param {
                type_: PARAM_OPENGL_INIT,
                data: &mut gl_init as *mut _ as *mut c_void,
            },
            mpv_render_param {
                type_: PARAM_INVALID,
                data: std::ptr::null_mut(),
            },
        ];

        let mut ctx: *mut mpv_render_context = std::ptr::null_mut();
        let rc =
            unsafe { mpv_render_context_create(&mut ctx, mpv.ctx.as_ptr(), params.as_mut_ptr()) };
        log::debug!("mpv_render_context_create {}", rc);
        if rc < 0 {
            return Err(MpvError::RenderContextCreate(rc));
        }

        unsafe {
            mpv_render_context_set_update_callback(
                ctx,
                Some(update_callback),
                Arc::as_ptr(&frame_ready) as *mut c_void,
            );
        }

        let gl = unsafe { GlObjects::new(width, height) };
        let fbo_tex = gl.tex;

        Ok(Self {
            ctx,
            width,
            height,
            fbo_tex,
            gl,
            frame_ready,
        })
    }

    /// Recreate the FBO when the output dimensions change.
    pub fn resize(&mut self, width: u32, height: u32) {
        if self.width == width && self.height == height {
            return;
        }
        self.width = width;
        self.height = height;

        let gl = unsafe { GlObjects::new(width, height) };
        self.fbo_tex = gl.tex;
        self.gl = gl; // drops the old FBO/texture via Drop
    }

    /// Render the next mpv frame into the internal FBO.
    ///
    /// Returns `true` if a frame was rendered, `false` if mpv had nothing new.
    pub fn render_frame(&mut self) -> bool {
        let flags = unsafe { mpv_render_context_update(self.ctx) };
        let update = flags & UPDATE_FRAME as u64 != 0;
        let ready = self.frame_ready.load(Ordering::Acquire);

        if !update && !ready {
            return  false;
        }

        let mut fbo_info = mpv_opengl_fbo {
            fbo: self.gl.fbo as i32,
            w: self.width as i32,
            h: self.height as i32,
            internal_format: gl::RGBA8 as i32,
        };

        let mut params = [
            mpv_render_param {
                type_: PARAM_OPENGL_FBO,
                data: &mut fbo_info as *mut _ as *mut c_void,
            },
            mpv_render_param {
                type_: PARAM_INVALID,
                data: std::ptr::null_mut(),
            },
        ];

        unsafe {
            mpv_render_context_render(self.ctx, params.as_mut_ptr());
        }

        self.frame_ready.store(false, Ordering::Release);
        true
    }

    pub fn frame_ready(&self) -> &Arc<AtomicBool> {
        &self.frame_ready
    }
}

impl Drop for GpuRenderContext {
    fn drop(&mut self) {
        // `self.gl` (FBO + texture) is freed automatically by `GlObjects::drop`.
        if !self.ctx.is_null() {
            unsafe { mpv_render_context_free(self.ctx) };
            self.ctx = std::ptr::null_mut();
        }
    }
}