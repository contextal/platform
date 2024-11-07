//! Foreign Function Interface to system iconv
#[cfg(not(target_os = "linux"))]
#[link(name = "iconv")]
extern "C" {}

#[allow(non_camel_case_types)]
pub type iconv_t = *mut ::std::os::raw::c_void;
extern "C" {
    pub fn iconv_close(__cd: iconv_t) -> ::std::os::raw::c_int;
}
extern "C" {
    pub fn iconv_open(
        __tocode: *const ::std::os::raw::c_char,
        __fromcode: *const ::std::os::raw::c_char,
    ) -> iconv_t;
}
extern "C" {
    pub fn iconv(
        __cd: iconv_t,
        __inbuf: *mut *const ::std::os::raw::c_char,
        __inbytesleft: *mut usize,
        __outbuf: *mut *mut ::std::os::raw::c_char,
        __outbytesleft: *mut usize,
    ) -> usize;
}
