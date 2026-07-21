# macOS App Store screenshot plan

## Technical requirements

- Provide between 1 and 10 screenshots.
- Use the same 16:10 size for both localizations.
- Recommended working size: `2880 × 1800` PNG.
- Also accepted: `2560 × 1600`, `1440 × 900`, or `1280 × 800`.
- Existing website captures at `1440 × 810` are 16:9 and cannot be reused directly as Mac App Store screenshots.
- Capture a Release build without debug overlays, personal server addresses, usernames, tokens, private file paths, or copyrighted media that is not cleared for marketing.

## Recommended six-frame sequence

1. **Home**
   - Show five continue-watching cards, libraries, and recently added content.
   - Chinese caption: `你的媒体库，一目了然`
   - English caption: `Your media, at a glance`

2. **Media detail**
   - Show artwork, season selection, episodes, cast, and media badges.
   - Chinese caption: `从剧集到资源，信息完整呈现`
   - English caption: `Every detail, from episodes to resources`

3. **Native player**
   - Show active video, readable subtitles, progress, audio/subtitle selectors, and next episode.
   - Chinese caption: `原生播放，音轨字幕自由切换`
   - English caption: `Native playback with audio and subtitles`

4. **Search**
   - Show mixed movie, series, and episode results.
   - Chinese caption: `快速找到想看的内容`
   - English caption: `Find what you want to watch`

5. **Multiple servers**
   - Show the server list with at least two non-sensitive demo configurations.
   - Chinese caption: `一个客户端，连接多个 MOVA 服务`
   - English caption: `One app, multiple MOVA servers`

6. **Server management**
   - Show libraries, a successful scan state, and notification center without private paths.
   - Chinese caption: `建库、扫描与通知实时同步`
   - English caption: `Libraries, scans, and notices in sync`

## Capture checklist

- Use a dedicated review/marketing server populated only with cleared sample media.
- Set the App language to match each localization before capture.
- Use consistent window dimensions and sidebar state.
- Keep the app chrome visible; do not composite unsupported controls or features.
- Avoid promises such as “all formats” or “every device.”
- Check all visible timestamps, filenames, user names, paths, and notifications before export.
