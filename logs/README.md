# logs

Everything the running system writes down. Nothing in here is committed.

| File | Written by | Holds |
|---|---|---|
| `backend.log` | the Axum server | requests, run lifecycle, database work, errors with their cause chain |
| `frontend.log` | the WebAssembly app | what the browser side did — clicks, fetches, render errors |
| `scraper.log` | the Google Maps scraper | raw output of each crawl, one line per place found |
| `wasm.log` | the build | `wasm-pack` / `trunk` output, kept so a failed build can be read after the fact |

The browser console is still the first place to look for anything visual. These files
are for what happens where you cannot see it.
