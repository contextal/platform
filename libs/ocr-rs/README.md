# A backend for performing optical character recognition #
The crate uses [Tesseract](https://github.com/tesseract-ocr/tesseract) library
via FFI to perform OCR functions.

The crate can be utilized as a usual backend service, which communicates with a
client via a network socket.

But it can be used as a "library" as well:
```
let mut api = TessBaseApi::new("eng", TessPageSegMode::PSM_AUTO, Some(Dpi(150)))
    .expect("failed to create Tesseract API instance");

api.set_rgba_image(&image).expect("failed to pass an image to Tesseract");

api.recognize().expect("text recognition step has failed");

let text = api.get_text().expect("failed to obtain recognized text");
```
In the example above `image` variable holds an image bitmap in `RGBA8`
representation.
