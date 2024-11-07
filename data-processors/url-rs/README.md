# A backend for gathering information about given URLs #
The backend relies on headless Chromium (or Chrome) browser to access provided
URL and its related objects and to collect information about these objects.

[chromiumoxide crate](https://docs.rs/chromiumoxide/latest/chromiumoxide/)
provides an interface to a headless browser via [Chrome DevTools
Protocol](https://chromedevtools.github.io/devtools-protocol/).

## The backend provides the following: ##
- contents of the given URL and its related objects (images, scripts,
  CSS-files, fonts, etc), which from browser's standpoint are necessary to
  render given URL
- main document HTML source code after it has been exposed as a subject for
  modification to JavaScript code
- rendered web page screenshot
- web page saved via print-to-PDF browser function
- file-download, if given URL triggers download process (i.e. `Menu` ->
  `Downloads`) in the browser
