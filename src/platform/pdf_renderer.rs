//! PDF page rasterization through Android's `PdfRenderer` (JNI), one PNG per
//! page at a readable resolution. The renderer is only used by the export
//! pipeline; keep this layer thin and dumb (CLAUDE.md).

use std::path::Path;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum PdfRenderError {
    #[cfg(target_os = "android")]
    #[error("Impossible de lire le PDF pour produire les images PNG.")]
    Android(#[from] jni::errors::Error),
    #[cfg(target_os = "android")]
    #[error("Impossible d'encoder une page PDF en image PNG.")]
    Encode,
    #[error("La lecture du PDF n'a produit aucune page.")]
    NoPages,
    #[error("La lecture des pages PDF n'est disponible que sur Android.")]
    Unsupported,
}

#[cfg(target_os = "android")]
const RENDER_DPI: i32 = 150;
#[cfg(target_os = "android")]
const POINTS_PER_INCH: i32 = 72;
#[cfg(target_os = "android")]
const MODE_READ_ONLY: i32 = 0x10000000;
#[cfg(target_os = "android")]
const RENDER_MODE_FOR_PRINT: i32 = 2;
#[cfg(target_os = "android")]
const WHITE: i32 = -1; // 0xFFFFFFFF as signed

/// Renders every page of `pdf_path` to PNG bytes at ~150 dpi.
pub fn render_pdf_to_pngs(pdf_path: &Path) -> Result<Vec<Vec<u8>>, PdfRenderError> {
    render_pdf_to_pngs_impl(pdf_path)
}

#[cfg(target_os = "android")]
fn render_pdf_to_pngs_impl(pdf_path: &Path) -> Result<Vec<Vec<u8>>, PdfRenderError> {
    use jni::JavaVM;

    let android = ndk_context::android_context();
    let vm = unsafe { JavaVM::from_raw(android.vm().cast()) }.map_err(PdfRenderError::Android)?;
    let mut env = vm
        .attach_current_thread()
        .map_err(PdfRenderError::Android)?;
    let result = render_all_pages(&mut env, pdf_path);
    if result.is_err() {
        // A pending Java exception becomes fatal when the native thread
        // detaches; clear it so the error surfaces as a French message only.
        let _ = env.exception_clear();
    }
    result
}

#[cfg(target_os = "android")]
fn render_all_pages(
    env: &mut jni::AttachGuard,
    pdf_path: &Path,
) -> Result<Vec<Vec<u8>>, PdfRenderError> {
    use jni::objects::{JObject, JValue};

    let path_string = env.new_string(pdf_path.to_string_lossy().as_ref())?;
    let file = env.new_object(
        "java/io/File",
        "(Ljava/lang/String;)V",
        &[JValue::Object(&path_string)],
    )?;
    let descriptor = env
        .call_static_method(
            "android/os/ParcelFileDescriptor",
            "open",
            "(Ljava/io/File;I)Landroid/os/ParcelFileDescriptor;",
            &[JValue::Object(&file), JValue::Int(MODE_READ_ONLY)],
        )?
        .l()?;
    let renderer = env.new_object(
        "android/graphics/pdf/PdfRenderer",
        "(Landroid/os/ParcelFileDescriptor;)V",
        &[JValue::Object(&descriptor)],
    )?;

    let result = (|| -> Result<Vec<Vec<u8>>, PdfRenderError> {
        let page_count = env
            .call_method(&renderer, "getPageCount", "()I", &[])?
            .i()?;
        if page_count <= 0 {
            return Ok(Vec::new());
        }

        let config = env
            .get_static_field(
                "android/graphics/Bitmap$Config",
                "ARGB_8888",
                "Landroid/graphics/Bitmap$Config;",
            )?
            .l()?;
        let png_format = env
            .get_static_field(
                "android/graphics/Bitmap$CompressFormat",
                "PNG",
                "Landroid/graphics/Bitmap$CompressFormat;",
            )?
            .l()?;

        let mut pages = Vec::with_capacity(page_count as usize);
        for index in 0..page_count {
            // Each iteration allocates ~4 local JNI refs; free them per page
            // instead of letting the table grow to the 512-entry abort.
            let bytes = env.with_local_frame(16, |env| -> Result<Vec<u8>, PdfRenderError> {
                let page = env
                    .call_method(
                        &renderer,
                        "openPage",
                        "(I)Landroid/graphics/pdf/PdfRenderer$Page;",
                        &[JValue::Int(index)],
                    )?
                    .l()?;
                let width_points = env.call_method(&page, "getWidth", "()I", &[])?.i()?;
                let height_points = env.call_method(&page, "getHeight", "()I", &[])?.i()?;
                let width = width_points.max(1) * RENDER_DPI / POINTS_PER_INCH;
                let height = height_points.max(1) * RENDER_DPI / POINTS_PER_INCH;

                let bitmap = env
                    .call_static_method(
                        "android/graphics/Bitmap",
                        "createBitmap",
                        "(IILandroid/graphics/Bitmap$Config;)Landroid/graphics/Bitmap;",
                        &[
                            JValue::Int(width),
                            JValue::Int(height),
                            JValue::Object(&config),
                        ],
                    )?
                    .l()?;
                // PdfRenderer leaves untouched pixels transparent; documents are
                // printed on white.
                env.call_method(&bitmap, "eraseColor", "(I)V", &[JValue::Int(WHITE)])?;
                let null = JObject::null();
                env.call_method(
                    &page,
                    "render",
                    "(Landroid/graphics/Bitmap;Landroid/graphics/Rect;Landroid/graphics/Matrix;I)V",
                    &[
                        JValue::Object(&bitmap),
                        JValue::Object(&null),
                        JValue::Object(&null),
                        JValue::Int(RENDER_MODE_FOR_PRINT),
                    ],
                )?;

                let stream = env.new_object("java/io/ByteArrayOutputStream", "()V", &[])?;
                // Quality is ignored for PNG; 100 is the conventional value.
                let compressed = env
                    .call_method(
                        &bitmap,
                        "compress",
                        "(Landroid/graphics/Bitmap$CompressFormat;ILjava/io/OutputStream;)Z",
                        &[
                            JValue::Object(&png_format),
                            JValue::Int(100),
                            JValue::Object(&stream),
                        ],
                    )?
                    .z()?;
                if !compressed {
                    return Err(PdfRenderError::Encode);
                }
                let bytes = env.call_method(&stream, "toByteArray", "()[B", &[])?.l()?;
                let bytes = env.convert_byte_array(jni::objects::JByteArray::from(bytes))?;

                env.call_method(&page, "close", "()V", &[])?;
                env.call_method(&bitmap, "recycle", "()V", &[])?;
                Ok(bytes)
            })?;
            pages.push(bytes);
        }
        Ok(pages)
    })();

    if result.is_err() {
        // A pending Java exception becomes fatal when the native thread
        // detaches; clear it before any further JNI call.
        let _ = env.exception_clear();
    }
    let pages = result?;
    env.call_method(&renderer, "close", "()V", &[])?;
    env.call_method(&descriptor, "close", "()V", &[])?;
    Ok(pages)
}

#[cfg(not(target_os = "android"))]
fn render_pdf_to_pngs_impl(_pdf_path: &Path) -> Result<Vec<Vec<u8>>, PdfRenderError> {
    Err(PdfRenderError::Unsupported)
}
