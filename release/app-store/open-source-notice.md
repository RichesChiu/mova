# Open-source notice for MOVA macOS

MOVA includes dynamically linked libraries from FFmpeg 8.1.2.

FFmpeg is licensed under the GNU Lesser General Public License version 2.1 or later. The MOVA distribution uses an LGPL-only configuration and does not include FFmpeg command-line programs, GPL components, external codec libraries, encoders, muxers, or filters.

- FFmpeg project: https://ffmpeg.org/
- Corresponding source archive: https://ffmpeg.org/releases/ffmpeg-8.1.2.tar.xz
- Source SHA-256: `464beb5e7bf0c311e68b45ae2f04e9cc2af88851abb4082231742a74d97b524c`
- License: GNU Lesser General Public License 2.1 or later

The distributed app bundle includes `FFmpeg-NOTICE.txt` and the complete `FFmpeg-LGPL-2.1.txt` license text. The FFmpeg libraries are placed in `Mova.app/Contents/Frameworks` as separately replaceable dynamic libraries.

FFmpeg is a trademark of Fabrice Bellard, originator of the FFmpeg project. MOVA is not endorsed by or affiliated with the FFmpeg project.

Questions about the bundled build or a corresponding-source request may be sent to `riches.chiu@gmail.com`.
