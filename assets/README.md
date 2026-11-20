# Brand assets

The visual identity: mascot, wordmark, icons, favicons, splash screens and book
covers. `brand/` is the asset pack unpacked; `brand/PACK-README.md` is the
designer's own notes, including print caveats.

The `.zip` the pack arrived in is gitignored — it is redundant with the unpacked
copy, and nothing (mdBook, a build script, an HTML page) can reference a file
inside an archive.

## Palette

These are the project's colours. Use them rather than inventing near-misses.

| Role | Hex | Where |
| --- | --- | --- |
| Dark brown | `#4C2311` | the mascot's outline; the "Quokka" wordmark |
| Quokka tan | `#E9974E` | the mascot's fur |
| Soft cream | `#FDF7EC` | light backgrounds, text on dark |
| Teal | `#105C55` | the "Lisp" wordmark; accents; `theme-color` |
| Dark teal | `#073A39` | dark backgrounds, the lambda mark |
| Mint | `#A2D9C5` | highlights on dark |

Companion choices, not sampled spot colours.

## What to reach for

| Need | File |
| --- | --- |
| Full logo, transparent | `brand/01_Logos/quokka-lisp-original-transparent.png` |
| Mascot alone | `brand/01_Logos/mascot-only-transparent.png` |
| Wordmark alone | `brand/01_Logos/wordmark-original-transparent.png` |
| Horizontal lockup | `brand/01_Logos/logo-horizontal-transparent-1440x360.png` |
| macOS app icon | `brand/02_Application_Icons/quokka-lisp-macos.icns` |
| Windows app icon | `brand/02_Application_Icons/quokka-lisp-windows.ico` |
| Linux app icon | `brand/02_Application_Icons/app-icon-256.png` |
| Favicons | `brand/03_Website_and_Favicons/` |
| Website hero | `brand/03_Website_and_Favicons/website-hero-1920x760.png` |
| Splash screens | `brand/04_Splash_Screens/` |
| Book cover drafts | `brand/05_Book_Covers/` |
| Everything at a glance | `brand/06_Brand_Guide/asset-pack-preview.png` |

## In use

- `book/theme/favicon.png` and `favicon.svg` — the book's tab icon
- `crates/quokka-wasm/www/` — the dev console's favicon and palette
- `README.md` — the horizontal lockup

## Notes

- `Quokka Lisp Mascot Logo.png` is the original standalone export. It is the
  same artwork and dimensions as `brand/01_Logos/quokka-lisp-original-transparent.png`
  but a different encoding, so it is kept rather than deduplicated — it is the
  file the pack was derived from. Reach for the one in `brand/` when building
  anything, so there is one path to change.

- **Raster only.** No editable vector paths; the pack's README says as much. The
  one exception is `book/theme/favicon.svg`, a plain lambda mark drawn by hand in
  the brand teal to match `favicon-lambda-*.png`, because a browser prefers an
  SVG favicon and scaling a PNG to 16px looks poor.
- **Book covers are front-only drafts** at 6×9in / 300dpi. A printer will want
  trim, bleed, spine width and a back cover before anything goes to press.
- **The cover and splash copy is provisional.** "A friendly playground for
  exploring language concepts" is the designer's placeholder, not a decided
  tagline.
- An early note in `docs/design.md` had the mascot's tail as a quote mark
  (`'quokka`). The artwork is the authority, so that note was corrected rather
  than the drawing. The lambda on the laptop carries the Lisp reference instead.
