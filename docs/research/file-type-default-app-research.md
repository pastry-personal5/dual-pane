# File types and default applications on macOS research

Status: Active

This note explains how macOS connects a file to the application that opens it, and how a default application is set, changed, and read. It records what was verified on a macOS 26.7.1 machine, what comes from Apple documentation, and what rests only on third-party reports. It is background for any future "Open With" or default-application feature; it decides nothing.

## Summary

1. A file does not name its application. macOS first resolves the file to a **content type** (a Uniform Type Identifier, UTI), then **Launch Services** picks an application that claims that type.
2. The choice has three layers: a per-file override, a per-user preference for the type, and the ranking of application claims.
3. Users change the default in Finder (**Get Info → Open with → Change All** for a type, **Open With → Always Open With** for one file). Programs change it through `NSWorkspace`.
4. Dual Pane today opens a file with `QDesktopServices::openUrl`, which reaches `-[NSWorkspace openURL:]`. It therefore follows the user's Finder defaults without storing any binding of its own.

## From extension to content type

- An extension is only a tag. A UTI such as `public.plain-text` or `com.adobe.pdf` is the identity. Types form a hierarchy through *conformance*: `net.daringfireball.markdown` conforms to `public.plain-text`, which conforms to `public.text`, `public.data`, and `public.item`. An application that handles a parent type can open its children.
- Apps declare types in `Info.plist` under `UTExportedTypeDeclarations` (the app owns the type) or `UTImportedTypeDeclarations` (the type is defined elsewhere). A declaration lists `UTTypeIdentifier`, `UTTypeConformsTo`, and `UTTypeTagSpecification`, whose `public.filename-extension` and `public.mime-type` entries map extensions and MIME types to the type. Launch Services collects these declarations from every registered application.
- An extension that no declaration claims gets a **dynamic** type named `dyn.…`. It matches no specific handler.
- The mapping is a system-wide database, not a property of the file. The same `.md` file can be `net.daringfireball.markdown` on one Mac and another type on a Mac with a different set of applications installed.

**Verified locally.** Resolving types with `UTType(filenameExtension:)` and `URLResourceValues.contentType` gave:

| Name | Content type | Dynamic |
|---|---|---|
| `README.md` | `net.daringfireball.markdown` | no |
| `Cargo.toml` | `public.toml` | no |
| `Makefile` | `public.make-source` | no |
| extension `xyzzy` | `dyn.ah62d4rv4ge81u8p4tm6u` | yes |

`mdls` reported the Markdown type tree as `net.daringfireball.markdown → public.plain-text → public.text → public.data → public.item → public.content`. The Launch Services dump shows the Markdown type is not declared by the system: it is an *imported* declaration inside application bundles (the dump lists it for the Claude app and Xcode), with tags `.md`, `.markdown`, and others. The type exists because installed applications declare it.

## How Launch Services picks the application

### Claims by applications

Each application lists the types it can open in `CFBundleDocumentTypes`. Useful keys are `LSItemContentTypes` (UTIs, preferred), `CFBundleTypeExtensions` (legacy extension matching), `CFBundleTypeRole` (`Editor`, `Viewer`, `Shell`, `None`), and `LSHandlerRank`.

Apple's archived [Launch Services release notes](https://developer.apple.com/library/archive/releasenotes/Carbon/RN-LaunchServices/index.html) define the rank: a missing `LSHandlerRank` means `Default`; `Owner` is for the primary handler of a custom type and is preferred over `Default` claims; `Alternate` marks a secondary editor or viewer that is not chosen over a better-ranked handler; `None` means never select the app for that type, though it can still accept drops. Those notes date from 2007 and do not give the full tie-breaking algorithm. Which application wins among equal-rank claims is **not documented** in anything this research found.

**Verified locally.** `lsregister -dump` shows these claims. A claim has `rank`, `roles` (for example `Viewer`), `flags: doc-type`, and `bindings` (extensions or UTIs). The dump listed 713 registered bundles on this machine.

### User choices

A user choice overrides the claims:

- **One file.** Finder's "Always Open With" (and Get Info → Open with, without Change All) records the choice for that file only. Apple's [`setDefaultApplication(at:toOpenFileAt:completion:)`](https://developer.apple.com/documentation/appkit/nsworkspace/setdefaultapplication(at:toopenfileat:completion:).md) documents the same scope: "a specific file (rather than all files of that content type)". A third-party write-up ([HackTricks](https://hacktricks.wiki/en/macos-hardening/macos-security-and-privilege-escalation/macos-file-extension-apps.html), reported through a search summary because the page itself could not be fetched) says this is stored as the extended attribute `com.apple.LaunchServices.OpenWith` on the file. That attribute name is **not verified**: no file here carries it, and creating one would change real defaults.
- **All files of a type.** Finder's **Change All** and `NSWorkspace.setDefaultApplication(at:toOpen:)` write a per-user entry for the content type. [Apple Support](https://support.apple.com/guide/mac-help/choose-an-app-to-open-a-file-on-mac-mh35597/mac) gives the user steps: select a file, Get Info, expand "Open with", choose an app, click **Change All**; Apple warns not to click Change All when only one file should change.

### Resolution order (best current understanding)

1. Per-file override on the file, if present (third-party evidence only).
2. The user's entry for the file's content type, then for the types it conforms to.
3. Application claims, ordered by rank, with undocumented tie-breaking.

One GitHub issue ([Warp #13268](https://github.com/warpdotdev/warp/issues/13268)) reports a UTI-level claim beating an extension-only claim even at `Alternate` rank. That is a user report, not an Apple statement, so treat step 3 as imprecise.

## Where the default is stored

| Item | Location | Notes |
|---|---|---|
| Per-user type and scheme bindings | `~/Library/Preferences/com.apple.LaunchServices/com.apple.launchservices.secure.plist`, key `LSHandlers` | Array of dictionaries. Verified locally. |
| Older per-user file | `~/Library/Preferences/com.apple.LaunchServices/com.apple.LaunchServices.plist` | Present on this Mac; the content was not inspected. |
| User election store | `~/Library/Preferences/com.apple.LaunchServices/com.apple.LaunchServices.SettingsStore.sql` | SQLite with tables `Election` and `LegacyElection` (`identifier`, `timestamp`, `userElection`). Both tables were empty here, so its role is unknown. |
| Application claims and type declarations | The Launch Services database, filled from each app's `Info.plist` at registration | Inspect with `lsregister -dump`. Not a file to edit. |

### The `LSHandlers` entry format

Each entry is keyed by `LSHandlerContentType` (a UTI) or `LSHandlerURLScheme`; both were observed locally. It then names the application by bundle identifier in a role key. Locally every entry used `LSHandlerRoleAll` with a bundle id, plus `LSHandlerModificationDate` and `LSHandlerPreferredVersions`. Other keys, such as a content-tag form, exist in older documentation and community posts but were not observed. A typical entry:

```
{
    LSHandlerContentType = "com.adobe.pdf";
    LSHandlerRoleAll = "<bundle id of the chosen app>";
    LSHandlerModificationDate = <seconds since 2001-01-01>;
    LSHandlerPreferredVersions = { LSHandlerRoleAll = "-"; };
}
```

Observations from this Mac:

- Entries exist for URL schemes (`http`, `https`, `mailto`, and custom app schemes), for PDF and HTML, and for pseudo-types such as `com.apple.default-app.web-browser` and `com.apple.default-app.mail-client`. The default web browser and mail client are stored as such pseudo-types, not as a real file type.
- There is **no** entry for Markdown, TOML, or `Makefile`. Their defaults (Xcode and a code editor) therefore come from Launch Services' ranking of application claims, not from a user choice. The user's Finder "Change All" would add an entry like the PDF one.
- Absence of an entry is normal; most types never get one.

Only `LSHandlerRoleAll` was observed here; per-role keys (viewer, editor, shell) are not confirmed on this machine. Do not edit the plist by hand: the daemon caches it, and community reports say a direct edit needs a Launch Services re-registration before it takes effect.

## Reading the default and the candidates

These AppKit calls (macOS 12 and later for the content-type forms) read the answer without touching Launch Services internals:

| Question | API |
|---|---|
| Default app for this file | `NSWorkspace.urlForApplication(toOpen: fileURL)` |
| All apps that can open this file | `NSWorkspace.urlsForApplications(toOpen: fileURL)` |
| Default app for a type | `NSWorkspace.urlForApplication(toOpen: UTType)` |
| Resolve extension to type | `UTType(filenameExtension:)` |
| Type of a real file | `URLResourceValues.contentType` |

**Verified locally** with a compiled Swift probe on this machine (the calls returned a default and a candidate list; the table shows counts and the default):

| File | Default app | Candidates |
|---|---|---|
| `README.md` | Xcode | 12 |
| `Cargo.toml` | Visual Studio Code | 7 |
| `Makefile` | Xcode | 10 |

For each file, the file-based lookup and the type-based lookup agreed, as expected when no per-file override exists. The candidate list is ordered with the default first. It includes apps that only claim a parent type, such as `public.plain-text`, which is why a Markdown file lists several general text editors.

## Setting the default

### Through the user interface

See [Apple Support](https://support.apple.com/guide/mac-help/choose-an-app-to-open-a-file-on-mac-mh35597/mac). One-time opening: drag the file onto the app, or use File → Open With. Permanent per type: Get Info → Open with → **Change All**. Finder's per-file "Always Open With" variant is not described on that page; it is known only from the per-file API and the third-party report above.

### Through AppKit

| API | Scope | Notes |
|---|---|---|
| `setDefaultApplication(at:toOpenFileAt:completion:)` | One file | macOS 12+. Needs write access to the file. If the change needs user consent, the system asks before calling the completion handler. File URLs only. |
| `setDefaultApplication(at:toOpen: UTType, completion:)` | All files of a type | macOS 12+. Chromium's [shell integration](https://chromium.googlesource.com/chromium/src/+/main/chrome/browser/shell_integration_mac.mm) uses it for HTML and passes empty completion handlers. |
| `setDefaultApplication(at:toOpenURLsWithScheme:completion:)` | A URL scheme | Used for `http` and `https`. |

An [Apple Developer Forums thread](https://developer.apple.com/forums/thread/731555) reports the type form failing with `NSOSStatusErrorDomain` -54 ("permErr") even with Full Disk Access, while the per-file form succeeds. An Apple engineer there says the failure is expected for a **sandboxed** app (the App Sandbox blocks changing application bindings) and that -54 is Launch Services' generic "permission denied". The thread shows no supported workaround and no resolution.

The Apple documentation pages for `CFBundleDocumentTypes` and the UTI declaration guide returned only page titles to the fetch tool, so the key descriptions above come from the 2007 release notes and from widely documented keys, not from a quoted current page.

### Through command-line tools

- [`duti`](https://sourceforge.net/projects/duti/) sets defaults by UTI or extension and by URL scheme. It was **not installed** here and its documentation could not be fetched, so its syntax and roles are not recorded. Tools of this kind work by calling Launch Services or by writing `LSHandlers`.
- `lsregister` (at `/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister`) dumps (`-dump`) and re-registers the database. It is an internal tool, so do not script against it for product behavior.
- `mdls -name kMDItemContentType -name kMDItemContentTypeTree <file>` shows a file's type and its conformance chain without side effects. It is the quickest way to see why a file maps to an application.

## Why a default can look wrong

- **No user choice exists.** Ranking and registration order decide, and installing an app that claims the type can change the winner. The Warp issue above is an example.
- **A per-file override exists.** Two files of the same type can open in different apps.
- **The type is dynamic.** An unknown extension has no declared claimants; only apps claiming `public.data` or similar will be offered.
- **A stale registration.** Community reports say a moved or deleted app can linger in Launch Services until it is re-registered. Not reproduced here.

## What Dual Pane does today

`open_with_default_application` in `crates/dual-pane-desktop/cpp/src/native_shell.cpp` calls `QDesktopServices::openUrl(QUrl::fromLocalFile(path))`. In Qt's Cocoa platform plugin, `QCocoaServices::openDocument` forwards to `openUrl`, which calls `[[NSWorkspace sharedWorkspace] openURL:]` (verified in the [`qcocoaservices.mm`](https://raw.githubusercontent.com/qt/qtbase/dev/src/plugins/platforms/cocoa/qcocoaservices.mm) source on the `dev` branch; the pinned Qt 6.11.2 was not checked separately). The result is a plain boolean, which matches the existing note that the call "reports only whether macOS accepted the request".

Consequences:

- The product follows the user's Finder defaults, per-file and per-type, with no code of its own. Changing a default in Finder changes Dual Pane immediately.
- The request carries no application choice, so Dual Pane cannot show or remember one today. Activating a file never lets the user pick an app. [product-behavior.md](../product-behavior.md) and [ux-gui.md](../ux-gui.md) describe only opening "with its default application".

## Inputs for a future Open With feature

These are options the research supports, not decisions. Any such feature touches an undecided product item and must be agreed first.

- **List candidates.** `urlsForApplications(toOpen:)` returns the apps that can open the selection, with the default first, so a menu needs no database access of its own.
- **Open with a chosen app.** `NSWorkspace.open(_:withApplicationAt:configuration:completionHandler:)` takes the file URLs and an application URL. It reports an error, which would give better failure reporting than the current boolean.
- **Show the current default.** `urlForApplication(toOpen:)` gives the name and icon for a menu or an inspector.
- **Change the default.** Use `setDefaultApplication(at:toOpenFileAt:)` or the type form. If Dual Pane runs unsandboxed (not checked here), the sandbox failure above should not apply, but the -54 report shows the type form is not guaranteed. A feature that edits defaults should surface the completion error and may prefer leaving type-wide changes to Finder's Get Info window.
- **Selections of mixed types.** A single default app does not exist for a mixed selection. A feature would need a rule, such as showing only apps that can open every selected file.
- **Testing.** Tests must use temporary directories, as in [AGENTS.md](../../AGENTS.md). Changing a default alters real user state, so such a test must not run against the real Launch Services database; it would need an opt-in script like `scripts/clipboard-acceptance.sh`.

## Open questions

- The exact tie-breaking among equal-rank claims is undocumented. Locally, only the outcome was observed, not the rule.
- The `com.apple.LaunchServices.OpenWith` attribute name and the per-file storage format are third-party claims, not confirmed on this Mac.
- The purpose of `SettingsStore.sql` (`Election` tables) is unknown; both tables were empty here.
- Whether Qt 6.11.2 matches Qt's `dev` branch for `QCocoaServices::openUrl` was not confirmed.

## Sources

- Apple: [Launch Services release notes](https://developer.apple.com/library/archive/releasenotes/Carbon/RN-LaunchServices/index.html) (archive, 2007), [`setDefaultApplication(at:toOpenFileAt:completion:)`](https://developer.apple.com/documentation/appkit/nsworkspace/setdefaultapplication(at:toopenfileat:completion:).md), [Choose an app to open a file on Mac](https://support.apple.com/guide/mac-help/choose-an-app-to-open-a-file-on-mac-mh35597/mac), [Developer Forums thread 731555](https://developer.apple.com/forums/thread/731555).
- Other: [Chromium `shell_integration_mac.mm`](https://chromium.googlesource.com/chromium/src/+/main/chrome/browser/shell_integration_mac.mm), [Qt `qcocoaservices.mm`](https://raw.githubusercontent.com/qt/qtbase/dev/src/plugins/platforms/cocoa/qcocoaservices.mm), [Warp issue #13268](https://github.com/warpdotdev/warp/issues/13268), [HackTricks file-extension handlers](https://hacktricks.wiki/en/macos-hardening/macos-security-and-privilege-escalation/macos-file-extension-apps.html), [duti](https://sourceforge.net/projects/duti/).
- Local, read-only, on macOS 26.7.1: `defaults read` of `LSHandlers`, `lsregister -dump`, `mdls`, `xattr -l`, `sqlite3 -readonly` of the settings store, and a compiled Swift probe of `NSWorkspace` and `UTType`.
