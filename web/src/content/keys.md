## How Your Keys Work

TinyCongress uses cryptographic keys instead of traditional passwords to prove
your identity. Your keys are generated in your browser and never leave your
device — the server only sees the public half, which can verify your identity
but can't impersonate you.

### Your root key

When you create an account, a **root key pair** is generated in your browser.
Think of it as your master identity — it's the ultimate proof that you own your
account. Your root key:

- Is created locally on your device, never on the server
- Signs and authorizes every device you log in from
- Can't be reset or recovered by anyone else (including us)

Because it's so important, TinyCongress encrypts a backup copy with your
**backup password** and stores the encrypted version on the server. The server
can't read it — only your password can unlock it.

### Your backup password

Your backup password protects the encrypted copy of your root key. You'll need
it when you log in on a new browser or device. Without it, there is no way to
recover your root key.

- Choose something strong and memorable
- TinyCongress cannot reset it for you
- It's only used during login to decrypt your root key — it's never sent to the
  server in plain text

### Device keys

Each browser or device you log in from gets its own **device key**. Device keys
are authorized by your root key (via a signed certificate), so the server knows
they belong to you.

- You can have up to 10 devices at a time
- Each device key is independent — revoking one doesn't affect the others
- You can manage your devices from the Settings page

### Why this matters

Most platforms store your credentials on their servers. If the server is
compromised, everyone's accounts are at risk. TinyCongress flips this: the
server is a **witness**, not an authority. It can verify your signatures but
can't forge them, which means:

- No one at TinyCongress can impersonate you or vote on your behalf
- A server breach can't leak your private keys (they were never there)
- Your identity is truly yours — backed by math, not trust
