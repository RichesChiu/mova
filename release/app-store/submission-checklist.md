# MOVA macOS 1.0 submission checklist

## Website and public URLs

- [ ] Deploy `https://mova.hk/privacy` and verify direct loading in a private browser window.
- [ ] Deploy `https://mova.hk/support` and verify the support email link.
- [ ] Confirm both pages switch between Simplified Chinese and English.
- [ ] Confirm the policy still matches the final submitted binary.

## App record

- [ ] Enroll in the Apple Developer Program as an individual.
- [ ] Register Bundle ID `hk.mova.client`.
- [ ] Create the App Store Connect record with primary language Simplified Chinese.
- [ ] Set macOS primary category and matching Xcode `LSApplicationCategoryType`.
- [ ] Add English (U.S.) localization.
- [ ] Enter support, marketing, and privacy policy URLs.
- [ ] Complete age rating, content rights, encryption, and App Privacy questionnaires.
- [ ] Complete China mainland availability and filing information when applicable.

## Binary

- [ ] Archive a Release build for both Apple silicon and Intel.
- [ ] Confirm bundled FFmpeg libraries resolve only inside the app bundle.
- [ ] Confirm the FFmpeg notice and LGPL license text are present in the archived app resources.
- [ ] Confirm Hardened Runtime, signing, entitlements, and sandbox choices on the archived binary.
- [ ] Validate the archive in Xcode Organizer.
- [ ] Test a distributed build on a clean Mac without Homebrew FFmpeg.

## Review environment

- [ ] Provision a stable public HTTPS MOVA demo server.
- [ ] Create a non-expiring administrator review account.
- [ ] Add legally cleared demo media that exercises video, audio, subtitles, episodes, and progress.
- [ ] Test every review-note step from an external network.
- [ ] Enter contact name, email, and telephone number.

## Product page

- [ ] Capture 1–10 screenshots at an accepted 16:10 Mac resolution.
- [ ] Upload separate Simplified Chinese and English screenshots/captions if text is localized.
- [ ] Paste and proofread localized metadata.
- [ ] Confirm keywords stay within the 100-byte limit.
- [ ] Use manual release for version 1.0 so approval does not publish unexpectedly.
