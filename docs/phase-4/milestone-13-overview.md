# P4-M13: File associations and Open With

Status: Active

This milestone adds File Associations to the Settings sidebar. It lets the user choose, for each macOS content type, which recommended applications appear as flat **Open with App Name** actions in the Normal and Comprehensive Folder Items Context Menus. Both menus offer **Configure Open With…** to reach the selected type's settings, while no app action appears until the user selects it. The current macOS user can also change that type's default application through the supported system API. The [architecture plan](milestone-13-architecture.md) defines the platform boundary, persistence, asynchronous discovery, and menu dispatch.

## Scope

In scope:

- Regular files and symbolic links that resolve to regular files. A selection of several files qualifies only when every file has the same resolved content type and an application is available for all of them.
- Types observed in already listed open Browser folders, plus types the user adds by choosing a sample file. Discovery does not recursively scan folders. A type with saved choices remains in Settings after its source folder closes.
- A new File Associations Settings page with the type's localized kind, extension when available, content-type identifier, current macOS default, and the system-ranked application candidates. Each candidate has independent Normal and Comprehensive checkboxes. Application names are disambiguated when necessary.
- Flat **Open with App Name** menu rows in the existing Open section, using the chosen type and menu variant, plus **Configure Open With…** in both menus to open File Associations Settings for that type. The ordinary **Open Item** command keeps its current default-app behavior.
- A separately confirmed **Set as Default** action for one recommended application and one content type. This changes the default for the current macOS user account, so Finder and other apps can follow it. It does not create a per-file override.
- Persistent Dual Pane menu choices, type pins, refresh and reset controls, accessible status and failure feedback, migration, and isolated verification.

Out of scope:

- Folders, packages, and links to packages; URL schemes, web-browser and mail-client defaults; per-file overrides; applications outside the macOS candidate list; custom menu ordering; and a built-in viewer.
- Changing defaults for other macOS accounts, editing Launch Services files directly, or claiming that every application honors a macOS default.
- Changing the existing keyboard shortcut editor or making each dynamically discovered application a shortcut-capable `ActionId`.

## Owner decisions — 2026-10-23

- The default-app setting is for the **current macOS user**, not all accounts. Dual Pane must confirm every requested change before asking macOS to apply it. The system may ask for its own consent as well.
- The recommended list is macOS's ranked available-app list. Users may enable several apps for one type, independently for Normal and Comprehensive. A newly discovered type has no app enabled in either menu; users choose the apps they want in File Associations Settings.
- Both Context Menus enable **Configure Open With…** by default for an eligible file selection so the user can go directly to that type's Settings page. It remains available after apps are chosen. Context Menus Settings may hide it independently in Normal and Comprehensive; File Associations Settings remains accessible from the Settings sidebar.
- **Open with App Name** entries are individual rows in the flat menu, not a submenu.
- Several selected files may use Open With when they have the same content type and at least one common available app.
- The Settings page discovers types represented by files in open folders and also offers **Add File Type…** using a sample file.
- This milestone covers regular files and links to regular files, not packages.

## Delivered interaction and policy

- Content-type identifiers, rather than filename extensions or localized Kind labels, key the saved choices. A type row shows enough detail to distinguish types that share a label or extension. An observed type disappears when it is no longer in any open listing only if it was never pinned or customized.
- Each menu variant starts with an empty selected-app set for a type. The candidate list is shown in Settings with all Normal and Comprehensive checkboxes clear; checking an app adds its flat row only to the chosen menu. Newly installed apps also start unchecked. Clearing every checkbox hides all app rows for that type and variant; **Configure Open With…** remains if that menu's Context Menus setting is enabled. A per-type **Clear Open With Choices…** control clears both sets after confirmation. Missing selected apps remain visible as unavailable in Settings so the user can remove or replace them; they are omitted from menus.
- The File Associations page is global to both Browsers. **Add File Type…** pins the sample file's type without saving the file path. Editing a type also keeps it listed after its source folder closes. **Remove Type…** forgets its Dual Pane pin and menu choices after a scope-specific confirmation; it never changes the macOS default. Global Reset Settings removes local pins and menu choices but does not undo changes already made to macOS defaults, and its confirmation explains that limit.
- Each context menu lists **Open Item**, then the eligible **Open with App Name** rows, then **Configure Open With…** when enabled, then the remaining Open-section actions. If the chosen app is also the current default, its explicit row may still appear. App rows use the macOS candidate ranking, without user-defined reordering. File Associations Settings owns app-row visibility; Context Menus Settings owns setup-action visibility. The setup action remains eligible when no app is selected.
- Opening the menu must remain responsive. Candidate and type discovery runs away from menu construction, with bounded work and a cache. If the relevant result is not ready, the setup action can open the Settings page with an identifying-type state; selected app rows appear on a later menu request when discovery completes. An open menu is not modified in place. A discovery failure is explained in Settings and does not invent candidates.
- Opening with a chosen app is a one-time action for the captured files. A selected link to a regular file opens its captured target, following the existing Open Item rule. It does not change the default or a per-file override. The system's asynchronous result determines success or failure; failures reach Notices. A menu result never silently retargets a new selection, tab, item, link target, type, or application.
- **Set as Default** shows the old and proposed app and states that the choice affects this content type for the current macOS user, including Finder. Cancel is the default and Escape action. After acceptance, show progress until the system completion and a fresh default lookup agree; cancellation, consent denial, and errors leave the prior displayed default until refreshed and show an actionable explanation. The control is unavailable when the selected app is missing or the same default. A per-file override can still make an individual file open differently.

## Completion checklist

- [x] Finalize the planned page labels, confirmation wording, and empty/loading/error states against the owner decisions above; update product behavior, UX GUI, information architecture, terms, and README with the delivered behavior.
- [x] Resolve real files and file links to exact macOS content-type identifiers, discover distinct types from open listings and a chosen sample file without recursive scanning, and fetch ranked candidate apps and the type default asynchronously. Type display, duplicate app names, missing apps, dynamic or unknown types, and external app/default changes have defined feedback.
- [x] Add File Associations as the fourth Settings category. Type selection, Add File Type…, initially clear independent app checkboxes, Clear Open With Choices…, Remove Type…, Refresh Applications, the current default, and Set as Default work by pointer and keyboard with accessible names, status, and errors. Settings-unavailable and global-reset behavior follows the existing settings lifecycle.
- [x] Persist only Dual Pane pins, per-type/per-menu app choices, and the new setup action's Context Menus visibility in a transactional schema migration. Preserve customized settings through relaunch and application-install changes, reject malformed data without partial writes, and migrate every supported older schema. Never store a sample file path or a copy of the macOS default as the authority.
- [x] Both Folder Items Context Menus enable Configure Open With… by default for an eligible file selection, opening File Associations Settings for its captured type before or after app selection. It can be hidden independently in Context Menus Settings. Only enabled, currently eligible flat app rows appear in the Open section, in macOS rank order, according to independent per-type choices. One file or same-type multiple files are eligible; folders, packages, mixed types, and no common app yield no app rows. Menu construction remains responsive while discovery is pending or fails.
- [x] A chosen row opens exactly the captured file URLs once with the chosen available application through the supported asynchronous macOS API. Dismissal, repeat invocation, refresh, selection or tab change, changed file identity or type, removed app, and late discovery cannot retarget or repeat the request. Opening errors reach Notices; no file contents are changed by Dual Pane.
- [x] Set as Default asks for scope-specific confirmation, calls macOS's type-wide API only after acceptance, handles the system's asynchronous consent and errors, reads the resulting default back, and refreshes the page and menu state. A cancellation or failure is visible and does not claim success. Global Reset Settings and Remove Type do not alter macOS defaults.
- [x] Qt-free tests cover initially empty per-type/menu choices, independent edits and clearing, unavailable saved-app removal, both setup-action visibility settings and dispatch, multi-file eligibility and rank, exact target capture, one-use and stale dispatch, asynchronous effect-result routing, and settings projections. Storage tests cover schema migration from every supported version including version 11's new action rows, fresh and customized round trips, malformed rows, rollback, relaunch, and reset. Native checks use disposable profiles and temporary files for type and app discovery, flat-menu behavior, setup navigation, responsiveness, and accessibility.
- [ ] Guided macOS acceptance verifies app names and ranking, both menus and Browsers, same-type multi-selection, type discovery and persistence, confirmation wording, external default visibility, and success and refusal of a default change in a disposable macOS account or another controlled environment. Automated checks never change the real user's Launch Services defaults; record any manual default change and restoration explicitly.
- [x] `make check` and the affected isolated native scripts pass after implementation. Record exact results and check every item before marking P4-M13 Done.

## Automated acceptance evidence — 2026-10-23

- `make check` passed all Rust and C++ formatting, lint, build, and test gates. `git diff --check` passed.
- `scripts/check-settings-window.sh`, `scripts/check-folder-items-context-menu.sh`, and `scripts/check-file-associations.sh` passed. The File Associations check used a disposable profile and temporary text files and covered native type discovery, non-blocking empty choices, Configure Open With navigation, and accessible controls. On this macOS profile Launch Services reported no candidate app for the fixture, so the check explicitly verified that safe empty state; candidate-dependent flat-row, independent-menu, and same-type multi-selection behavior remains covered by the application tests and runs in the native check whenever macOS supplies a candidate.
- Application and storage tests cover sparse preferences, unavailable-app removal, rank-preserving common apps, one-use and stale menu dispatch, exact source and resolved-target capture, asynchronous failure/refresh routing, migration through schema 12, transactional rollback, relaunch, malformed data, and reset.
- Automation did not invoke Open With or Set as Default and did not change the real user's Launch Services defaults. The guided macOS acceptance below remains open, so P4-M13 remains Active.

## Guided macOS acceptance

Use a disposable macOS account or another controlled environment. Record the account/environment and the original default before changing it; restore that original default at the end and record the successful restoration. Automated checks must not perform these steps.

- [ ] Open text and image fixtures in each Browser and confirm the localized type names, extensions, exact identifiers, macOS candidate ranking, duplicate-name disambiguation when available, and the current external default.
- [ ] Enable different apps in Normal and Comprehensive, verify both menus and same-type multi-selection, relaunch, and confirm that local choices and pinned sample types persist.
- [ ] From **Configure Open With…**, confirm the captured type opens even before an app is selected; verify mixed types, folders, packages, missing apps, and no-common-app selections do not gain an app row.
- [ ] Change one type default after reading the current-user/Finder warning, confirm macOS and an external app report the new default, then restore the original default. Also exercise Cancel and one refusal or consent-denial path and confirm Dual Pane does not claim success.

## Planning boundary

P4-M13 was planned while P4-M12 and P4-M7 still awaited guided physical-pointer acceptance. The owner's explicit implementation request on 2026-10-23 deliberately updated that sequence; it does not close or weaken those manual gates. The M10 flat-menu and no-live-update rules remain, with M13 adding dynamic rows to the Open section. [Apple's AppKit documentation](https://developer.apple.com/documentation/appkit/nsworkspace) supplies the supported candidate, open, and default-change APIs; the [research note](../research/file-type-default-app-research.md) records the local observations and their limits.
