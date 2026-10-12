# Light Assist

Remote assistance between two computers on the same network: one person shares their screen so that
another can see it or, with permission, use it. The person assisting installs nothing and needs only
a browser.

- **To get assistance:** install Light Assist on the computer that needs it, from
  [assist.mazzella.dev](https://assist.mazzella.dev), and choose *Start*. It shows an address to pass on.
- **To assist someone:** open the address they give you in a browser, and read them the code it shows.

The sharer decides everything: seeing the screen, using the mouse and keyboard, and a terminal are
each allowed separately and can be taken back at any time. What passes between the two computers goes
directly from one to the other, encrypted. See the [privacy policy](https://assist.mazzella.dev/privacy).

Windows and macOS can share; any modern browser can assist.

## Building it

You need Rust (1.88 or newer), Node.js and pnpm.

```
pnpm install
pnpm build                                   # the pages the app carries
cargo build --release -p light-assist        # the app, in target/release
```

On a Mac, `sh scripts/package-macos.sh` makes `Light Assist.app`. To build an app that uses the hosted
page, set `LIGHT_ASSIST_RELAY_URL` and `LIGHT_ASSIST_PAGE_URL` to the relay's and the website's addresses
when building it.

## On a Mac

A Mac shows an app the screen, and lets it use the mouse and keyboard, only once you have allowed it,
and lists only apps that someone has signed. Until there is a downloadable Mac app signed by its
developer, build your own copy, signed with your own certificate:

1. Install the Xcode command line tools (`xcode-select --install`), [Rust](https://rustup.rs),
   [Node.js](https://nodejs.org) and [pnpm](https://pnpm.io/installation).
2. Make a free certificate: in Xcode, Settings → Accounts → add your Apple ID → Manage Certificates →
   + → Apple Development.
3. In this folder, run `sh scripts/install-macos.sh`. It builds the app, signs it with that certificate,
   puts it in Applications and opens it.
4. In System Settings → Privacy & Security, allow Light Assist under Screen & System Audio Recording,
   then quit it and open it again. Accessibility is asked for the first time someone wants to use your
   mouse and keyboard.

To update, get the newer source and run the script again; what you allowed is kept.

## License

Copyright © 2026 Francesco Mazzella.

Light Assist is free software: you can redistribute it and modify it under the terms of the
[GNU Affero General Public License](LICENSE) as published by the Free Software Foundation, either
version 3 of the License, or (at your option) any later version. It comes with no warranty.

In short: you may use, study, change and share it, and if you distribute a changed version, or run
one as a service for other people, you must make your changes available under the same license.

The libraries it is built on keep their own licenses (MIT, Apache 2.0, BSD and similar, all of which
allow this).
