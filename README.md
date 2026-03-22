# media-viewer

[![GitHub Release][releases-shield]][releases]
![Project Stage][project-stage-shield]
[![GitHub License][license-shield]](LICENSE.md)
[![Github Actions][github-actions-shield]][github-actions]
![Project Maintenance][maintenance-shield]
[![GitHub Activity][commits-shield]][commits]

## About

media-viewer is a cross platform application to browse media collections.
The app design is based on Google Picasa, which is no longer available, so I needed a replacement, for browsing and sorting my pictures.

## Features

- add multiple source folders
- Grid view to browse all supported formats
- Thumbnail generation for fast browsing
- Resize the display size of the thumbnails within the grid view

## Needed dependencies

- [libheif](https://github.com/strukturag/libheif) is needed to create thumbnails for HEIC/HEIF files
  * install on macOS: `brew install libheif`
  * install on ubuntu: `sudo apt-get install -y libheif1`
  * install on windows: `vcpkg install libheif`

## Optional Dependencies

- [ffmpeg](https://ffmpeg.org/) is used to created thumbnails for video files
  * install on macOS: `brew install ffmpeg`
  * install on ubuntu: `sudo apt-get install -y ffmpeg`
  * install on windows: `vcpkg install ffmpeg`

## Support

Got questions?

Feel free to [open an issue here][issue] on GitHub.

## Authors & contributors

The original setup of this repository is by [Matthias Balke][matthiasbalke].

For a full list of all authors and contributors, check [the contributor's page][contributors].

## MIT License

Copyright (c) 2026 Matthias Balke

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

[project-stage-shield]: https://img.shields.io/badge/project%20stage-early%09testing-yellow.svg
[license-shield]: https://img.shields.io/github/license/matthiasbalke/media-viewer
[releases-shield]: https://img.shields.io/github/v/release/matthiasbalke/media-viewer?include_prereleases
[releases]: https://github.com/matthiasbalke/media-viewer/releases
[github-actions-shield]: https://github.com/matthiasbalke/media-viewer/actions/workflows/build-tauri-app.yaml/badge.svg
[github-actions]: https://github.com/matthiasbalke/media-viewer/actions

[maintenance-shield]: https://img.shields.io/maintenance/yes/2026.svg
[commits-shield]: https://img.shields.io/github/commit-activity/y/matthiasbalke/media-viewer.svg
[commits]: https://github.com/matthiasbalke/media-viewer/commits/main

[issue]: https://github.com/matthiasbalke/media-viewer/issues
[matthiasbalke]: https://github.com/matthiasbalke
[contributors]: https://github.com/matthiasbalke/media-viewer/graphs/contributors
