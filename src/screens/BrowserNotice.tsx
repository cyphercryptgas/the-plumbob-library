import { PRODUCT_NAME, PRODUCT_TAGLINE } from "../lib/product";
import { Card, PlumbobMark } from "../components/ui";

/** Shown when the interface runs in a plain browser instead of the shell. */
export function BrowserNotice() {
  return (
    <main className="flex h-full items-center justify-center bg-app p-8">
      <Card className="max-w-lg">
        <div className="flex items-center gap-3">
          <PlumbobMark size={36} />
          <div>
            <h1 className="text-lg font-semibold text-ink">{PRODUCT_NAME}</h1>
            <p className="text-sm text-ink-secondary">{PRODUCT_TAGLINE}</p>
          </div>
        </div>
        <p className="mt-4 rounded-control bg-blue-soft p-4 text-sm leading-relaxed text-muted-blue-deep">
          You're viewing the interface in a regular browser, so there's no
          library data and no file operations here — that all lives in the
          desktop app. Install the Windows build from the latest
          "Windows Installer" run on the repository's Actions tab.
        </p>
      </Card>
    </main>
  );
}
