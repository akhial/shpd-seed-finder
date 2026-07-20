import { useCallback, useEffect, useRef, useState } from 'react'

// Owner/repo of the GitHub project whose releases host the native builds.
const REPO = 'akhial/shpd-seed-seeker'
const RELEASES_URL = `https://github.com/${REPO}/releases/latest`
const API_URL = `https://api.github.com/repos/${REPO}/releases/latest`

interface ReleaseAsset {
  name: string
  browser_download_url: string
}

interface Release {
  tag_name: string
  assets: ReleaseAsset[]
}

interface DownloadOption {
  archLabel: string
  name: string
  url: string
}

interface PlatformDef {
  key: string
  label: string
  // Returns an architecture label if the asset is one of this platform's app
  // builds, or null otherwise. The CLI archives are filtered out beforehand.
  match(name: string): string | null
}

// Release asset names embed the tag, e.g. `seed-seeker-v0.5.0-windows-x64.zip`.
// Each platform matches its GUI-app artifacts and maps the arch token to a
// human label; ordering here sets the order shown in the modal.
const PLATFORMS: PlatformDef[] = [
  {
    key: 'windows',
    label: 'Windows',
    match: (name) => {
      const m = /-windows-([a-z0-9]+)\.zip$/.exec(name)
      if (!m) return null
      return m[1] === 'x64' ? 'x64 · Intel/AMD' : m[1] === 'arm64' ? 'ARM64' : m[1]
    },
  },
  {
    key: 'macos',
    label: 'macOS',
    match: (name) => {
      const m = /-macos-([a-z0-9_]+)\.app\.zip$/.exec(name)
      if (!m) return null
      return m[1] === 'arm64' ? 'Apple Silicon' : m[1] === 'x86_64' ? 'Intel' : m[1]
    },
  },
  {
    key: 'linux',
    label: 'Linux',
    match: (name) => {
      const m = /-([a-z0-9_]+)\.AppImage$/.exec(name)
      if (!m) return null
      return m[1] === 'x86_64' ? 'x86_64 · Intel/AMD' : m[1] === 'aarch64' ? 'ARM64 · aarch64' : m[1]
    },
  },
  {
    key: 'android',
    label: 'Android',
    match: (name) => (/-android(?:-unsigned)?\.apk$/.test(name) ? 'Universal APK' : null),
  },
]

function optionsFor(platform: PlatformDef, release: Release): DownloadOption[] {
  return release.assets
    // CLI archives (`seed-seeker-cli-…`) share platform tokens; skip them so
    // only the graphical-app builds surface under these links.
    .filter((asset) => !asset.name.includes('-cli-'))
    .map((asset) => {
      const archLabel = platform.match(asset.name)
      return archLabel ? { archLabel, name: asset.name, url: asset.browser_download_url } : null
    })
    .filter((option): option is DownloadOption => option !== null)
}

function triggerDownload(url: string) {
  const anchor = document.createElement('a')
  anchor.href = url
  anchor.rel = 'noopener'
  // Ignored cross-origin, but release assets ship a Content-Disposition
  // attachment header, so the browser downloads rather than navigates.
  anchor.download = ''
  document.body.appendChild(anchor)
  anchor.click()
  anchor.remove()
}

interface ModalState {
  platform: PlatformDef
  options: DownloadOption[]
}

export function DownloadMenu() {
  const [busy, setBusy] = useState<string | null>(null)
  const [modal, setModal] = useState<ModalState | null>(null)
  // Cache the release lookup so repeated clicks don't re-hit the API.
  const releasePromise = useRef<Promise<Release | null> | null>(null)

  const loadRelease = useCallback((): Promise<Release | null> => {
    releasePromise.current ??= fetch(API_URL, { headers: { Accept: 'application/vnd.github+json' } })
      .then((response) => (response.ok ? (response.json() as Promise<Release>) : null))
      .catch(() => null)
    return releasePromise.current
  }, [])

  const onPlatform = useCallback(
    async (platform: PlatformDef) => {
      setBusy(platform.key)
      const release = await loadRelease()
      setBusy(null)
      const options = release ? optionsFor(platform, release) : []
      if (options.length === 0) {
        // No release yet, network/API failure, or no matching asset — fall
        // back to the releases page so the download is still reachable.
        window.open(RELEASES_URL, '_blank', 'noopener')
        return
      }
      if (options.length === 1) {
        triggerDownload(options[0].url)
        return
      }
      setModal({ platform, options })
    },
    [loadRelease],
  )

  const closeModal = useCallback(() => setModal(null), [])

  useEffect(() => {
    if (!modal) return
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') closeModal()
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [modal, closeModal])

  return (
    <>
      <span className="d1-avail">
        <span className="d1-avail-lead">Seed Seeker is also available on…</span>
        <span className="d1-avail-links">
          {PLATFORMS.map((platform) => (
            <button
              key={platform.key}
              type="button"
              className="d1-avail-link"
              disabled={busy !== null}
              aria-busy={busy === platform.key}
              onClick={() => void onPlatform(platform)}
            >
              {platform.label}
            </button>
          ))}
        </span>
      </span>

      {modal && (
        <div
          className="d1-overlay"
          onMouseDown={(event) => {
            if (event.target === event.currentTarget) closeModal()
          }}
        >
          <div
            className="d1-modal d1-dl-modal"
            role="dialog"
            aria-modal="true"
            aria-label={`Download Seed Seeker for ${modal.platform.label}`}
          >
            <header className="d1-modal-head">
              <div className="d1-modal-title">
                <h2>Download for {modal.platform.label}</h2>
              </div>
            </header>

            <div className="d1-modal-body">
              <ul className="d1-dl-list">
                {modal.options.map((option) => (
                  <li key={option.url}>
                    <a
                      className="d1-dl-option"
                      href={option.url}
                      download
                      rel="noopener"
                      onClick={closeModal}
                    >
                      <span className="d1-dl-arch">{option.archLabel}</span>
                      <span className="d1-dl-file d1-mono">{option.name}</span>
                    </a>
                  </li>
                ))}
              </ul>
            </div>

            <footer className="d1-modal-foot">
              <a className="d1-btn" href={RELEASES_URL} target="_blank" rel="noopener">
                All releases
              </a>
              <button type="button" className="d1-btn d1-btn-primary" onClick={closeModal}>
                Close
              </button>
            </footer>
          </div>
        </div>
      )}
    </>
  )
}
