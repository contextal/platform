# ISO9660 and UDF backend #

This backend extracts files and metadata from popular CD, DVD, etc images.

### ISO9660 support ###
Canonical (typically .iso) and raw (.raw, .img, .bin, .nrg) formats with
or without a custom header are supported

The header presence and the raw sector size are autodetected

Joliet extensions are supported; RockRidge extensions are currently NOT supported.

### UDF support ###
All legal block sizes are supported and automatically detected. 

Files using allocation descriptor continuation are currently NOT supported.
