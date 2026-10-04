//! Web-sized JPGs straight from the camera's own preview, via macOS ImageIO.
//!
//! Canon CR3s (and most other RAWs) carry a full-resolution JPEG the camera
//! rendered when the photo was taken. Downscaling that is ~20× faster than
//! decoding the RAW, and the result looks like it did on the camera's screen.
//! If a file has no preview big enough, we decode the RAW itself instead.

use std::ffi::c_void;
use std::path::Path;
use std::ptr;

use core_foundation::base::{CFRelease, CFType, CFTypeRef, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
use core_foundation::number::CFNumber;
use core_foundation::string::{CFString, CFStringRef};
use core_foundation::url::{CFURL, CFURLRef};

type CGImageSourceRef = *const c_void;
type CGImageDestinationRef = *const c_void;
type CGImageRef = *const c_void;

#[link(name = "ImageIO", kind = "framework")]
extern "C" {
    static kCGImageSourceCreateThumbnailFromImageIfAbsent: CFStringRef;
    static kCGImageSourceCreateThumbnailFromImageAlways: CFStringRef;
    static kCGImageSourceThumbnailMaxPixelSize: CFStringRef;
    static kCGImageSourceCreateThumbnailWithTransform: CFStringRef;
    static kCGImageDestinationLossyCompressionQuality: CFStringRef;
    static kCGImagePropertyExifDictionary: CFStringRef;
    static kCGImagePropertyExifDateTimeOriginal: CFStringRef;

    fn CGImageSourceCreateWithURL(url: CFURLRef, options: CFDictionaryRef) -> CGImageSourceRef;
    fn CGImageSourceCreateThumbnailAtIndex(src: CGImageSourceRef, index: usize, options: CFDictionaryRef) -> CGImageRef;
    fn CGImageSourceCopyPropertiesAtIndex(src: CGImageSourceRef, index: usize, options: CFDictionaryRef) -> CFDictionaryRef;
    fn CGImageDestinationCreateWithURL(url: CFURLRef, kind: CFStringRef, count: usize, options: CFDictionaryRef) -> CGImageDestinationRef;
    fn CGImageDestinationAddImage(dest: CGImageDestinationRef, image: CGImageRef, properties: CFDictionaryRef);
    fn CGImageDestinationFinalize(dest: CGImageDestinationRef) -> bool;
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGImageGetWidth(image: CGImageRef) -> usize;
    fn CGImageGetHeight(image: CGImageRef) -> usize;
}

// ImageIO autoreleases internally; our worker threads have no pool of their own.
#[link(name = "objc")]
extern "C" {
    fn objc_autoreleasePoolPush() -> *mut c_void;
    fn objc_autoreleasePoolPop(pool: *mut c_void);
}

/// A CoreFoundation object we own and release on drop.
struct Owned(*const c_void);

impl Owned {
    fn new(ptr: *const c_void) -> Option<Self> {
        (!ptr.is_null()).then_some(Self(ptr))
    }
}

impl Drop for Owned {
    fn drop(&mut self) {
        unsafe { CFRelease(self.0 as CFTypeRef) }
    }
}

fn key(k: CFStringRef) -> CFString {
    unsafe { CFString::wrap_under_get_rule(k) }
}

fn dict(pairs: &[(CFString, CFType)]) -> CFDictionary<CFString, CFType> {
    CFDictionary::from_CFType_pairs(pairs)
}

/// Writes `out` as a JPEG whose longest edge is `max_edge` (or the photo's
/// own size, if smaller). Returns false if the file couldn't be read or written.
pub fn web_jpeg(raw: &Path, out: &Path, max_edge: u32, quality: f64) -> bool {
    unsafe {
        let pool = objc_autoreleasePoolPush();
        let ok = web_jpeg_inner(raw, out, max_edge, quality).is_some();
        objc_autoreleasePoolPop(pool);
        ok
    }
}

unsafe fn web_jpeg_inner(raw: &Path, out: &Path, max_edge: u32, quality: f64) -> Option<()> {
    let src_url = CFURL::from_path(raw, false)?;
    let src = Owned::new(CGImageSourceCreateWithURL(src_url.as_concrete_TypeRef(), ptr::null()))?;

    let thumbnail = |source: CFStringRef| {
        let options = dict(&[
            (key(source), CFBoolean::true_value().as_CFType()),
            (key(kCGImageSourceThumbnailMaxPixelSize), CFNumber::from(max_edge as i64).as_CFType()),
            (key(kCGImageSourceCreateThumbnailWithTransform), CFBoolean::true_value().as_CFType()),
        ]);
        Owned::new(CGImageSourceCreateThumbnailAtIndex(src.0, 0, options.as_concrete_TypeRef()))
    };
    let long_edge = |image: &Owned| CGImageGetWidth(image.0).max(CGImageGetHeight(image.0));

    let mut image = thumbnail(kCGImageSourceCreateThumbnailFromImageIfAbsent);
    if image.as_ref().is_none_or(|i| long_edge(i) < max_edge as usize) {
        // No preview, or only a small one: decode the RAW itself.
        if let Some(decoded) = thumbnail(kCGImageSourceCreateThumbnailFromImageAlways) {
            if image.as_ref().is_none_or(|i| long_edge(&decoded) > long_edge(i)) {
                image = Some(decoded);
            }
        }
    }
    let image = image?;

    let mut properties = vec![(
        key(kCGImageDestinationLossyCompressionQuality),
        CFNumber::from(quality).as_CFType(),
    )];
    // Keep "date taken" so Pixieset can sort by it. Nothing else (no GPS).
    if let Some(taken) = date_taken(&src) {
        let exif = dict(&[(key(kCGImagePropertyExifDateTimeOriginal), taken)]);
        properties.push((key(kCGImagePropertyExifDictionary), exif.as_CFType()));
    }

    let out_url = CFURL::from_path(out, false)?;
    let jpeg = CFString::from_static_string("public.jpeg");
    let dest = Owned::new(CGImageDestinationCreateWithURL(out_url.as_concrete_TypeRef(), jpeg.as_concrete_TypeRef(), 1, ptr::null()))?;
    CGImageDestinationAddImage(dest.0, image.0, dict(&properties).as_concrete_TypeRef());
    CGImageDestinationFinalize(dest.0).then_some(())
}

unsafe fn date_taken(src: &Owned) -> Option<CFType> {
    let props = CGImageSourceCopyPropertiesAtIndex(src.0, 0, ptr::null());
    if props.is_null() {
        return None;
    }
    let props: CFDictionary<CFString, CFType> = CFDictionary::wrap_under_create_rule(props);
    let exif = props.find(key(kCGImagePropertyExifDictionary))?.downcast::<CFDictionary>()?;
    let exif: CFDictionary<CFString, CFType> = CFDictionary::wrap_under_get_rule(exif.as_concrete_TypeRef());
    exif.find(key(kCGImagePropertyExifDateTimeOriginal)).map(|v| v.clone())
}
