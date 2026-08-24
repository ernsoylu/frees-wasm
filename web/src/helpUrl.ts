// Deploy-base-aware links into the /help portal.
//
// Wave E fixed the help ROUTE (main.tsx resolves it against
// import.meta.env.BASE_URL), but every link SOURCE still hardcoded the
// origin-root '/help' — the desktop rail, the status pill's diagnostic deep
// links, Getting Started, Spotlight, the editor's F1 lookup and the mobile
// menu — so under a `vite build --base` sub-path deploy each of them pointed
// at a page that is not there. Found by the 2026-08-24 mobile-consistency
// audit (the mobile menu was the first sighting; the sweep found the rest).
// `hash` carries its own '#'.
export function helpUrl(hash = ''): string {
  return `${import.meta.env.BASE_URL.replace(/\/$/, '')}/help${hash}`
}
