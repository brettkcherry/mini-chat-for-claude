# Third-Party Notices

Mini Chat for Claude is [MIT-licensed](./LICENSE). It also bundles
open-source code from other projects, listed below. This file exists because
MIT and Apache-2.0 both require their license text and copyright notice to
travel with binary distributions — this is that notice, not a courtesy.

Scope: only dependencies that actually ship — either compiled into
`claude-mini.exe` or bundled into the frontend's JS output. Build-time-only
tooling (Vite, the Tauri CLI, PostCSS, etc.) never reaches a user's machine
and isn't listed here.

**No strong copyleft.** Nothing here is GPL, AGPL, or LGPL. Most of what's
below is permissive (MIT/Apache-2.0/BSD-style) or, for the two non-standard
ones, was individually reviewed and carries no obligation beyond attribution.
Five Rust crates are MPL-2.0 — weak, file-level copyleft: they ship unmodified
from crates.io, so the obligation is satisfied by the source links in the
per-crate report below, and linking against them imposes nothing on this
project's own code.

---

## Frontend (npm)

Three direct dependencies ship in the built frontend bundle; none of them
carry further runtime dependencies of their own.

| Package | Version | License |
|---|---|---|
| [dompurify](https://github.com/cure53/DOMPurify) | 3.4.12 | MPL-2.0 OR Apache-2.0 |
| [marked](https://github.com/markedjs/marked) | 18.0.5 | MIT |
| [@tauri-apps/api](https://github.com/tauri-apps/tauri) | 2.11.0 | Apache-2.0 OR MIT |

Where a package offers a choice of license, this project relies on the
MIT/Apache-2.0 branch — both are reproduced below.

<details>
<summary>MIT License</summary>

```
MIT License

Copyright (c) various — see each package above for its specific holder(s)

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to
deal in the Software without restriction, including without limitation the
rights to use, copy, modify, merge, publish, distribute, sublicense, and/or
sell copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.
```

</details>

<details>
<summary>Apache License 2.0</summary>

The full text is reproduced once, alongside the Rust dependencies that also
rely on it, in [`licenses/rust-third-party.html`](./licenses/rust-third-party.html#Apache-2.0)
rather than duplicated here — it runs to roughly 200 lines.

</details>

---

## Backend (Rust / Cargo)

547 crates are resolved into `src-tauri/Cargo.lock` and compiled into
`claude-mini.exe`. Listing each one's full license text inline here would
make this file unusable, so the complete, per-crate attribution — name,
version, license, copyright holders, and the full text of every license in
use — lives in a separate, generated file:

**[`licenses/rust-third-party.html`](./licenses/rust-third-party.html)**

Open it in a browser; it's grouped by license, with the crates using each one
listed underneath.

License families present across those 547 crates: `MIT`, `Apache-2.0`
(including the `WITH LLVM-exception` variant, and combinations naming
`BSD-2-Clause`/`BSD-3-Clause`/`ISC`/`Zlib`/`0BSD`/`Unlicense`/`CC0-1.0`/
`MIT-0`/`BSL-1.0`/`LGPL-2.1-or-later` as alternatives), `BSD-3-Clause`,
`ISC`, `Zlib`, `Unicode-3.0`, `MPL-2.0`, and `CDLA-Permissive-2.0`. Every
crate offering a choice of license is covered here by MIT or Apache-2.0.

### Regenerating this file

The Rust license report is generated, not hand-maintained. After any
dependency change:

```bash
cargo install cargo-about --locked --features cli
cd src-tauri
cargo about generate about.hbs -o ../licenses/rust-third-party.html --manifest-path Cargo.toml
```

`src-tauri/about.toml` is the accept-list `cargo-about` checks every
resolved license against — it deliberately fails the build if a crate update
introduces a license not already reviewed and listed there, rather than
silently including something with different obligations (e.g. copyleft).
If it ever rejects a license, that's the signal to open this file and look,
not to add the license to the accept-list without reading it first.
