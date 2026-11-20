# Quokka Lisp — Visual Identity Asset Pack

These assets were adapted from the approved **Quokka Lisp** transparent mascot logo. The original artwork is preserved; resized and recomposed variants are suitable for software, websites, documentation and book artwork. Raster images are PNG (not editable vector paths).

## Quick start

- **Application (macOS):** `02_Application_Icons/quokka-lisp-macos.icns`
- **Application (Windows):** `02_Application_Icons/quokka-lisp-windows.ico`
- **Application (Linux):** `02_Application_Icons/app-icon-256.png` or `app-icon-512.png`
- **Web browser:** `03_Website_and_Favicons/favicon.ico` plus the PNG favicons
- **iOS / PWA:** `03_Website_and_Favicons/apple-touch-icon.png` and `android-chrome-*.png`
- **Android adaptive icon:** foreground/background PNGs under `02_Application_Icons/`
- **Primary app splash:** `04_Splash_Screens/splash-light-1920x1080.png` or dark equivalent
- **Mobile splash:** `04_Splash_Screens/splash-*-mobile-1080x1920.png`
- **Book front cover designs:** `05_Book_Covers/` (1800×2700 px, 6×9 in at 300 dpi, **front only**)
- **Website hero / banner:** `03_Website_and_Favicons/website-hero-1920x760.png`
- **Original transparent logo:** `01_Logos/quokka-lisp-original-transparent.png`

## Palette (coordinating design colours)

| Role | Hex |
|---|---|
| Dark brown | `#4C2311` |
| Quokka tan | `#E9974E` |
| Soft cream | `#FDF7EC` |
| Teal | `#105C55` |
| Dark teal | `#073A39` |
| Mint | `#A2D9C5` |

These are companion palette choices, not exact sampled Pantone spot colours.

## Website usage example

Put the files from `03_Website_and_Favicons/` in your website's public root, then add:

```html
<link rel="icon" href="/favicon.ico" sizes="any">
<link rel="icon" type="image/png" sizes="32x32" href="/favicon-32x32.png">
<link rel="icon" type="image/png" sizes="16x16" href="/favicon-16x16.png">
<link rel="apple-touch-icon" href="/apple-touch-icon.png">
<link rel="manifest" href="/site.webmanifest">
<meta name="theme-color" content="#105C55">
```

The default browser favicon uses the complete quokka at 32px and above, and a lambda at 16px; additional lambda variants (`favicon-lambda-*`) are included for extra-small UIs.

## Print and editing notes

- Print cover PNGs are **front-cover artwork drafts** at 6×9 inches / 300 dpi. Before printing, your printer will need trim, bleed, spine width and back-cover dimensions. No author name has been invented.
- The copy on the covers/splashes is provisional branding language and can be changed.
- The splash-screen images are static: no fake live loading progress bar is embedded.
- Original mascot and wordmark remain available as separate transparent assets.
- No true editable vector tracing is claimed; request an SVG redraw if true vector paths are required for very large print or sign-making.
- For a tiny 16px icon, consider the `favicon-lambda-16.png` variant if the mascot appears too detailed.
