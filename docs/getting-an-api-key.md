# Getting an Anthropic API key

Mini Chat for Claude needs an **Anthropic API key** to work. This is different
from a Claude.ai (claude.ai) subscription — the API key is a separate,
pay-as-you-go credential tied to its own account balance. This guide assumes
no prior experience with Anthropic's website or with API keys in general.

## 1. Create an Anthropic account

1. Go to **[console.anthropic.com](https://console.anthropic.com/)** — this is
   the *developer console*, not the claude.ai chat website.
2. Click **Sign up** (or **Log in** if you already have a claude.ai account —
   the same login works for both).
3. Verify your email address if prompted.

## 2. Add billing / credits

The API is billed per use, separately from any claude.ai subscription. You
won't be able to send messages from Mini Chat until there's a positive
balance on the account.

1. In the console, open **Settings → Billing** (or **Plans & Billing**,
   depending on the current site layout).
2. Add a payment method and purchase credits — a **few dollars is enough** to
   get started; short conversations cost a fraction of a cent, and even
   frequent daily use tends to add up slowly.
3. Optional: set a **usage limit** or **spend alert** so you're notified
   before it grows larger than expected.

## 3. Create an API key

1. In the console, open **Settings → API Keys**.
2. Click **Create Key**.
3. Give it a name you'll recognize later (e.g. `mini-chat-for-claude`) and
   confirm.
4. The key is shown **once**, as a string starting with `sk-ant-...`. Click
   the copy button immediately — if you navigate away without copying it,
   you'll need to create a new key.

Treat this key like a password:

- Don't paste it into chat messages, screenshots, or public repositories.
- Don't share it with anyone else — each key can spend against your billing
  balance.
- If you ever suspect a key has leaked, delete it from **Settings → API
  Keys** and create a new one.

## 4. Paste it into Mini Chat for Claude

1. Open Mini Chat for Claude.
2. Click the 🔑 button.
3. Paste the key you copied in step 3.

The key is stored in **Windows Credential Manager**, not as plain text on
disk — see the main [README](../README.md#requirements) for details on how
it's used and what data leaves your machine.

## Troubleshooting

- **"Invalid API key" error** — double-check you copied the whole string
  (they're long) and that there's no extra whitespace at the start or end.
- **"Insufficient credits" / billing error** — go back to
  **Settings → Billing** in the console and confirm a payment method and
  balance are set up.
- **Key not accepted after pasting** — try creating a fresh key; keys can be
  revoked or expire if regenerated elsewhere.
