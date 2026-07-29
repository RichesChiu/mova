# MOVA macOS App Store submission

This is the working release checklist and App Review template for the native macOS client. Apple
requirements and every implementation-dependent statement must be checked against App Store
Connect and the exact binary at submission time.

## Public pages and App record

- [ ] Deploy `https://mova.hk/privacy` and verify direct loading in a private browser window.
- [ ] Deploy `https://mova.hk/support` and verify the support contact.
- [ ] Confirm both pages switch between Simplified Chinese and English.
- [ ] Confirm the privacy policy matches the submitted binary.
- [ ] Register Bundle ID `hk.mova.client`.
- [ ] Create the App Store Connect record with Simplified Chinese as the primary language.
- [ ] Select the macOS categories and set the matching Xcode `LSApplicationCategoryType`.
- [ ] Add English (U.S.) localization.
- [ ] Enter support, marketing, and privacy policy URLs.
- [ ] Complete age rating, content rights, encryption, App Privacy, regional availability, and
      filing requirements that apply at submission time.

Localized product copy is maintained in:

- [`metadata-zh-Hans.md`](metadata-zh-Hans.md)
- [`metadata-en-US.md`](metadata-en-US.md)

## Binary and open-source compliance

- [ ] Archive the intended Release build for every supported Mac architecture.
- [ ] Confirm bundled FFmpeg libraries resolve only inside the app bundle.
- [ ] Confirm the FFmpeg notice and LGPL license text are present in the archive.
- [ ] Revalidate every version, checksum and bundle-layout statement in
      [`open-source-notice.md`](open-source-notice.md).
- [ ] Confirm Hardened Runtime, signing, entitlements, sandbox choices, and export-compliance
      answers against the archived binary.
- [ ] Validate the archive in Xcode Organizer.
- [ ] Test a distributed build on a clean Mac without development-only dependencies.

## Review environment

MOVA requires a server and account. Do not submit until App Review has a stable public HTTPS demo
environment that has been tested outside the developer's local network.

- [ ] Provision a stable public HTTPS MOVA demo server.
- [ ] Create a review administrator account that remains valid throughout review.
- [ ] Use only media that the developer owns, created, or may legally distribute for review.
- [ ] Exercise video, audio tracks, subtitles, episodes, progress, scans, and notifications.
- [ ] Test every review-note step from an external network.
- [ ] Enter the real demo URL and credentials only in App Store Connect.
- [ ] Enter the required review contact name, email, and telephone number.

Never commit working review credentials to this repository. Do not use a local-network address,
expiring one-time password, or production account containing private media.

## Suggested App Review Notes

MOVA is a native client for a user-selected, self-hosted MOVA media server. The app includes no
media content and does not provide a public streaming catalog.

To review the app:

1. Launch MOVA and choose Add Server.
2. Enter the demo server URL and administrator credentials supplied in the Sign-in Required fields.
3. Open Home to review libraries, recently added titles, and continue watching.
4. Open a series to review seasons, episodes, cast, and media resource details.
5. Play a demo item to test seeking, audio tracks, subtitles, and playback progress.
6. Open Server Settings to review users, libraries, scan progress, and notifications.

The demo server is reachable over public HTTPS and will remain online during review. The supplied
account is an administrator so submitted administrative functionality can be tested.

Privacy notes:

- The app has no advertising, tracking, or developer-operated analytics.
- Credentials are sent directly to the selected MOVA server.
- Access and refresh tokens are stored in the macOS Keychain.
- Server configuration and preferences are stored locally on the Mac.
- Local Network permission is used only when the user selects a server on the same local network.

Revalidate these notes if the client adds analytics, crash reporting, proxying, cloud services, or
changes credential storage.

## Screenshots

Verify the current screenshot count, dimensions, formats, and localization rules in App Store
Connect before capture. Use one consistent 16:10 working canvas; `2880 × 1800` PNG is the preferred
source size for the current plan.

Capture a Release build without debug overlays, personal server addresses, usernames, tokens,
private paths, or uncleared media. Use consistent window dimensions and sidebar state.

Recommended sequence:

1. **Home**
   - Chinese: `你的媒体库，一目了然`
   - English: `Your media, at a glance`
2. **Media detail**
   - Chinese: `从剧集到资源，信息完整呈现`
   - English: `Every detail, from episodes to resources`
3. **Native player**
   - Chinese: `原生播放，音轨字幕自由切换`
   - English: `Native playback with audio and subtitles`
4. **Search**
   - Chinese: `快速找到想看的内容`
   - English: `Find what you want to watch`
5. **Multiple servers**
   - Chinese: `一个客户端，连接多个 MOVA 服务`
   - English: `One app, multiple MOVA servers`
6. **Server management**
   - Chinese: `建库、扫描与通知实时同步`
   - English: `Libraries, scans, and notices in sync`

Do not composite controls or advertise functionality absent from the submitted client. Check all
visible timestamps, filenames, account names, paths, notifications, and media rights before export.

## Final submission

- [ ] Proofread both metadata localizations in App Store Connect.
- [ ] Confirm keywords remain within App Store Connect limits.
- [ ] Upload localized screenshots and verify their order.
- [ ] Retain the completed privacy and export-compliance answers with the release record.
- [ ] Use manual release for the initial public version so approval does not publish unexpectedly.
