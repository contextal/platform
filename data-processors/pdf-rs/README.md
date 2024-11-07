# A backend for analyzing and extracting contents of PDF documents. #

The backend relies on `pdfium-render` crate, which provides Rust bindings for
`PDFium` C++ library to operate on PDF documents.

## The backend extracts/provides the following data: ##
- document's PDF standard version
- PDF document builtin metadata
- list of font names used in the document
- PDF form type
- hashes of embedded page thumbnails
- document page/paper dimensions
- counters for various types of PDF annotations, links, objects, attachments, cryptographic
signatures and bookmarks
- rendered document pages
- document text obtained from (optional) text objects in a PDF document
- document text produced by performing OCR on rendered document pages
- text from annotations
- image objects from document pages
- files attached to PDF document
- cryptographic signatures of PDF document
