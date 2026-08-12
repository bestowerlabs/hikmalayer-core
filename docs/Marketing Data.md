# Marketing Data — Hikmalayer

**A public information pack.** Everything here is safe to share: on websites, in
press releases, on social media, in investor and partner conversations, and in
interviews.

Written in plain language on purpose. If you can't explain it simply, people
won't trust it.

**Last updated:** August 2026 · **Status:** Pre-launch (see §12 — please read
that section before making any public claim)

---

## Table of contents

1. [The one-line version](#1-the-one-line-version)
2. [What is Hikmalayer, in plain words](#2-what-is-hikmalayer-in-plain-words)
3. [The big idea: getting ready for quantum computers](#3-the-big-idea-getting-ready-for-quantum-computers)
4. [What you can actually do with it](#4-what-you-can-actually-do-with-it)
5. [HKM — the coin](#5-hkm--the-coin)
6. [HTS — creating your own token](#6-hts--creating-your-own-token)
7. [The built-in exchange](#7-the-built-in-exchange)
8. [How it stays secure](#8-how-it-stays-secure)
9. [What makes it different](#9-what-makes-it-different)
10. [For developers](#10-for-developers)
11. [The people behind it](#11-the-people-behind-it)
12. [Being honest: what it can't do yet](#12-being-honest-what-it-cant-do-yet)
13. [Common questions](#13-common-questions)
14. [Ready-to-use copy](#14-ready-to-use-copy)
15. [Facts and figures](#15-facts-and-figures)
16. [Words to use and avoid](#16-words-to-use-and-avoid)
17. [Contact](#17-contact)

---

## 1. The one-line version

> **Hikmalayer is a blockchain built to survive quantum computers — and to prove
> certificates and qualifications are real, without ever putting your personal
> documents online.**

If you only have ten seconds, that's the sentence.

---

## 2. What is Hikmalayer, in plain words

### 2.1 What is a blockchain, quickly

Imagine a notebook that thousands of computers around the world each keep a
copy of. When someone writes a new line in it, every computer checks the line
is valid and adds it to their own copy. Nobody can secretly erase or change an
old line, because everyone else's copy would disagree.

That's a blockchain. Hikmalayer is one.

### 2.2 What makes Hikmalayer its own thing

Most blockchains are copies or extensions of Bitcoin or Ethereum. Hikmalayer
isn't. It was built from the ground up, in a programming language called Rust,
with its own address format, its own security rules, and its own coin.

The word for this is **Layer 1** — meaning it is a base blockchain in its own
right, not something built on top of someone else's.

### 2.3 The two things it's designed to do

**One: prove that documents are genuine.** Diplomas, certificates, licences,
training records, professional qualifications. Hikmalayer can prove a document
is real and hasn't been altered — without the document itself ever going online.

**Two: let people create and trade digital assets.** Anyone can create their own
token on Hikmalayer and trade it, using the exchange that's built into the
blockchain itself.

### 2.4 Why "Hikmalayer"?

"Hikma" (حكمة) is the Arabic word for **wisdom**. "Layer" refers to being a
Layer 1 blockchain. Wisdom as the foundation layer.

---

## 3. The big idea: getting ready for quantum computers

**This is the headline. It's what makes Hikmalayer worth talking about.**

### 3.1 The problem, explained simply

Every blockchain today — Bitcoin, Ethereum, all of them — protects your money
with a kind of digital signature. Only you can create your signature, because
only you have your secret key. Everyone else can check that the signature is
genuine.

The maths behind those signatures is very hard for normal computers to break.
It would take longer than the age of the universe.

**But quantum computers are different.** A big enough quantum computer could
break that maths in hours, not eons. There's a known method for doing it — it's
called Shor's algorithm, and it was published back in 1994. The maths isn't a
secret. The only missing piece is a machine powerful enough to run it.

When that machine exists, anyone who has it could take money from any wallet on
almost every blockchain in the world.

### 3.2 Why waiting is not an option

Here's the part most people miss:

> **The attack has already started, even though the machine doesn't exist yet.**

Someone can copy the blockchain today — it's all public — store it, and wait.
The day a quantum computer works, they open their saved copy and start taking
things. The information they need is already public and already saved.

Security people call this **"harvest now, decrypt later."** You can't fix it
afterwards. You have to fix it before.

### 3.3 Hikmalayer's answer: two locks instead of one

Think of your blockchain wallet as a safe with a lock on it.

- **Every other blockchain:** one lock. It's a very good lock. But quantum
  computers can pick that type of lock.
- **Hikmalayer:** you can choose an account with **two locks of completely
  different kinds**. One is the normal lock everyone uses. The other is a new
  kind that quantum computers are not able to pick.

**To open a Hikmalayer safe, a thief has to pick both locks.** A quantum
computer can pick the first one. It cannot pick the second. So the safe stays
shut.

And it works in reverse too. If someone one day finds a weakness in the *new*
lock, the *old* lock is still there holding the door.

**Your account stays safe as long as either lock holds.** That's the whole
idea, and it's why the design is called **hybrid** — it uses both, not one
replacing the other.

### 3.4 The second-lock technology

The new lock is called **ML-DSA-65**. It comes from **FIPS 204**, an official
standard published by NIST — the US government body that sets cryptography
standards for the world.

This matters. Hikmalayer didn't invent its own quantum-proof maths and hope for
the best. It uses the standard that the world's cryptographers reviewed,
competed over for years, and finalised in 2024.

### 3.5 Two kinds of address

| Address starts with | What it means |
|---|---|
| **hkm…** | Normal account. One lock. Fast and small |
| **hkq…** | Quantum-ready account. Two locks. Larger and slower, but safe against quantum computers |

You choose which to use. Both work on the same blockchain. Anyone can send money
to either kind.

### 3.6 The honest trade-off

Two locks cost more than one:

| | Normal account | Quantum-ready account |
|---|---|---|
| Size of each transaction | About 130 bytes | About 5,400 bytes (~40× bigger) |
| Time to sign in a browser | Under 1 millisecond | About 11 milliseconds |

That's the real price of quantum-proof security today. It's why Hikmalayer lets
you choose, rather than forcing everyone to pay it.

Eleven milliseconds is still faster than you can blink. But it's honest to say
it's slower.

### 3.7 The part everyone else gets wrong

Adding a second lock sounds easy. Doing it *correctly* is not, and this is worth
explaining because it's genuinely clever.

**The trap:** if you only write the first lock's details on the safe's label, a
thief who picks the first lock could show up with their *own* second lock, fit
it to the door, and claim the safe. Two locks, but one break opens it. The second
lock was decoration.

**Hikmalayer's fix:** the address is created by mixing **both** keys together.
Swap either one and you get a completely different address — a different safe
entirely. The thief's key doesn't open your account; it just describes an empty
account nobody owns.

This is checked and re-checked automatically every single time. There are tests
in the code that play the role of a quantum attacker holding a stolen key, and
prove the blockchain says no.

### 3.8 Where the protection applies

Not just when you send money. Everywhere it matters:

| Action | Protected? |
|---|---|
| Sending HKM | ✅ Both locks |
| Sending or creating tokens | ✅ Both locks |
| Trading on the exchange | ✅ Both locks |
| Issuing certificates | ✅ Both locks |
| Staking (locking coins to help run the network) | ✅ Both locks |
| **Getting your staked coins back** | ✅ Both locks |
| **Creating new blocks as a validator** | ✅ Both locks |

The last two matter more than they look. A validator's key sits in public view
for as long as their coins are staked — it's the most exposed key on the whole
network. Many designs would protect the wallet balance and forget the stake.
Hikmalayer protects both.

---

## 4. What you can actually do with it

### 4.1 Prove a certificate is genuine — without publishing it

This is Hikmalayer's flagship feature. It's called **Proof-of-Credential**.

**The problem it solves.** A university issues a degree certificate. An employer
wants to know it's real. Today that means phoning the university, waiting days,
and hoping someone answers. Meanwhile fake certificates are easy to buy.

**How Hikmalayer does it.** The university takes the certificate and creates a
**digital fingerprint** of it — a short code that only that exact document can
produce. Change a single letter and the fingerprint changes completely. Only
that fingerprint goes onto the blockchain.

Then anyone can check a certificate in seconds: create the fingerprint of the
document they're holding, compare it with the one on the blockchain. If they
match, the document is genuine and unaltered.

**The important part: your certificate never goes online.** Not your name, not
your grades, not your photograph. Only the fingerprint. And a fingerprint can't
be turned back into the document — it's a one-way street.

**It can be cancelled.** If a qualification is revoked — fraud, expiry,
misconduct — the issuer cancels it on the blockchain instantly, and every future
check shows it as cancelled. A certificate that can't be cancelled isn't a
certificate, it's just a receipt.

**Nobody has to be trusted.** The person checking doesn't have to trust
Hikmalayer, or the university's website, or any company. The maths proves it.

**Who this is for:** universities and colleges · professional bodies · training
providers · employers checking applicants · licensing authorities · government
departments.

### 4.2 Send and receive money

Send HKM to anyone, anywhere, in seconds. No bank, no business hours, no
borders.

### 4.3 Create your own token

Anyone can create their own digital token on Hikmalayer with a single
transaction. See §6.

### 4.4 Trade without a middleman

The exchange is built into the blockchain itself. No company holds your money
while you trade. See §7.

### 4.5 Lock coins on a schedule

If a project promises "the team's coins are locked for two years", Hikmalayer
can **enforce** that rather than just promise it. The coins physically cannot
move until the date arrives, and anybody can check the schedule.

A promise in a blog post can be broken. A lock in the blockchain cannot.

---

## 5. HKM — the coin

**HKM** is Hikmalayer's own coin, the way ETH is Ethereum's.

### 5.1 What it's for

1. **Paying fees.** Every action on Hikmalayer costs a small fee in HKM.
2. **Securing the network.** People lock up HKM to help run the blockchain
   (called *staking*) and earn rewards for doing it honestly.
3. **Trading.** Every trading pair on the built-in exchange uses HKM on one side.

### 5.2 The numbers

| | |
|---|---|
| Name | HKM |
| Total supply | About 100 billion HKM |
| Available at launch | 30 billion (30%) |
| Released over time as rewards | About 70 billion (70%) |
| Smallest unit | 0.000001 HKM |
| Reward per block at the start | 3,700 HKM |
| Rewards halve every | 9,500,000 blocks (roughly 4.5 years) |
| Long-term reward | 50 HKM per block, forever |

### 5.3 Why rewards halve

Like Bitcoin, new coins are created slowly and the rate halves over time. This
means the supply grows fast at the beginning, when the network needs to attract
people to run it, and slowly later, when it doesn't.

### 5.4 Why there's a permanent small reward

Bitcoin's rewards eventually stop completely. Hikmalayer keeps a small permanent
reward of 50 HKM per block.

The reason is simple: the people securing the network need a reason to keep
doing it. If rewards drop to zero and fees are low that year, security gets
weak exactly when nobody's watching. A small permanent reward means there's
always a budget to keep the network safe.

It does mean a tiny amount of new HKM forever. That's a deliberate choice, and
it's stated openly rather than hidden.

### 5.5 Nobody can print more

The release schedule is enforced by the blockchain itself. If someone changed
their software to pay themselves extra, every other computer would reject their
blocks. There is no "admin" who can create coins.

---

## 6. HTS — creating your own token

**HTS** stands for **Hikmalayer Token Standard**. It's how anyone creates their
own token.

### 6.1 One transaction

Send one transaction saying what you want — name, symbol, how many, how many
decimal places — and your token exists. No coding, no approval, no waiting.

### 6.2 What makes HTS tokens different

On most blockchains, a token is a small program written by whoever made it. That
program can contain nasty surprises: a hidden button to create unlimited new
coins, a blocklist that freezes your wallet, a switch that stops you selling.
People lose money to these constantly, and the only way to check is to hire an
expert to read the code.

**On Hikmalayer, tokens aren't programs.** They're built into the blockchain
itself. Which means **every** HTS token automatically has these guarantees:

| Guarantee | What it means |
|---|---|
| ✅ Supply fixed at creation | Nobody can create more later. Not even the creator |
| ✅ No blocklist | Nobody can freeze your wallet |
| ✅ No hidden switch | Nobody can stop you selling |
| ✅ No secret admin | There's no owner with special powers |
| ✅ Nothing to upgrade | The rules can't be changed after you buy |

You don't have to trust the token's creator or pay for an audit. The blockchain
enforces all of it, for every token, always.

**The honest flip side:** because tokens aren't programs, they also can't do
clever custom things. If you want a token with unusual built-in behaviour,
Hikmalayer isn't the right place. Simple and safe, or flexible and risky — this
design picks the first.

---

## 7. The built-in exchange

Hikmalayer has a **decentralised exchange (DEX)** built into the blockchain
itself.

### 7.1 How it works, simply

Instead of matching buyers with sellers, there's a shared pool of two things —
say HKM and a token. When you trade, you put one in and take the other out, and
the price adjusts automatically based on how much of each is left.

The people who supply the pool earn **0.3% of every trade** that passes through
it, split according to how much they contributed.

### 7.2 Why "built into the blockchain" matters

Most decentralised exchanges are programs running on top of a blockchain — and
programs can have bugs. Hundreds of millions of dollars have been lost to bugs
in exchange programs.

Hikmalayer's exchange is part of the blockchain's own rules. There is no program
to have a bug in.

### 7.3 Protection against bad prices

Every trade includes a limit: "don't complete this if I'd get less than X." If
the price moves badly before your trade goes through, it simply doesn't happen.
The developer tools won't even let you build a trade without this protection.

---

## 8. How it stays secure

### 8.1 Two systems agreeing, not one

Most blockchains pick one of two methods to decide who writes the next page:

- **Proof of Work** — computers compete by solving hard puzzles (Bitcoin)
- **Proof of Stake** — people who lock up coins take turns (Ethereum today)

**Hikmalayer uses both, in sequence:**

1. **Proof of Stake picks who** writes the next page — chosen fairly, weighted
   by how many coins they've locked up.
2. **Proof of Work proves they did it** — that chosen person then has to solve
   the puzzle before their page counts.

**The result:** having powerful computers is worthless on its own. If you don't
have coins locked up, you're never chosen, so you can never write a page no
matter how much computing power you buy. This blocks the classic "51% attack"
that pure Proof-of-Work chains worry about.

### 8.2 Fair selection nobody can rig

Who gets picked is decided by a method that produces a result nobody can predict
or influence. A validator can't try different options to improve their chances —
there's only ever one possible answer for them, and everyone can verify it.

### 8.3 The network can't get stuck

If the chosen validator is offline, the system waits 30 seconds and lets the
next one go. One person's computer failing delays things briefly. It can't stop
the network.

### 8.4 Cheating is punished

If a validator tries to write two conflicting pages, **anyone** can report it,
and the cheat automatically loses their locked coins. They also can't withdraw
their coins and run — coins stay locked and punishable for a waiting period
after a withdrawal request.

### 8.5 Every computer checks everything

Every computer on the network re-does every calculation in every block. If one
computer lies about a balance or a certificate, all the others catch it
immediately. Nobody has to be trusted.

### 8.6 Your keys stay yours

**The Hikmalayer software has no way to accept your secret key. Not through any
feature, not for any reason.** You sign things on your own device. Only the
signature travels.

### 8.7 Wallets built carefully

- Your key is stored **scrambled with strong encryption**, protected by your
  password. Without the password, the stored data is useless.
- While your wallet is unlocked, the key is kept in a protected form that even
  code running in the same web page cannot read.
- **You approve every single signature**, and you see exactly what you're
  approving. Nothing is ever signed silently.
- A browser extension is available that keeps your key completely outside any
  website. Even a hacked website can't reach it.
- Everything locks automatically when you're idle or close the tab.

### 8.8 Tested like an attacker

Hikmalayer has a special set of tests where the computer plays the *attacker*.
Each test tries to steal money, fake a signature, create coins from nothing, or
break the quantum protection — and checks that the blockchain says no.

The quantum tests are the strictest of all. They assume the attacker **already
has your key** — which is exactly what a quantum computer would give them — and
prove the account still holds.

**Current test coverage:** 139 core tests, 40 attacker tests, 58 wallet
compatibility tests, 21 live network tests, plus a full example application.
Every one passes.

---

## 9. What makes it different

| | Hikmalayer | Most blockchains |
|---|---|---|
| **Safe from quantum computers** | ✅ Yes, available now | ❌ Not yet |
| **Two locks on your account** | ✅ Optional per account | ❌ One only |
| **Certificates built in** | ✅ Part of the blockchain | ❌ Needs an add-on program |
| **Documents stay private** | ✅ Only a fingerprint is stored | Varies |
| **Exchange built in** | ✅ Part of the blockchain | ❌ Separate programs |
| **Tokens can't hide nasty surprises** | ✅ Guaranteed for all tokens | ❌ Depends on each token |
| **Powerful computers alone can attack** | ❌ Impossible without coins | ⚠️ Possible on some chains |
| **Team coin locks enforced** | ✅ By the blockchain | ⚠️ Often just a promise |

**The one-sentence pitch:** *Most blockchains will need rebuilding when quantum
computers arrive. Hikmalayer was built for that world from the start.*

---

## 10. For developers

### 10.1 Getting started takes one command

```bash
ops/devnet.sh
```

That gives you a complete working blockchain on your own computer — funded
account, running validator, blocks being produced.

### 10.2 A simple, clean toolkit

```js
import { HikmalayerClient, HybridSigner, parseUnits } from "@hikmalayer/sdk";

// Quantum-ready account — exactly the same code, two signatures behind the scenes
const client = HikmalayerClient.withHybridPrivateKey(process.env.KEY);

await client.transfer({ to: "hkq…", amount: parseUnits("1.5") });
await client.createAsset({ symbol: "MYTOKEN", decimals: 6, initialSupply: parseUnits("1000000") });
await client.swap({ tokenId, hkmToToken: true, amountIn: parseUnits("5") });
```

Switching to quantum-ready protection is **one line of code**. That was
deliberate — security that's hard to turn on doesn't get turned on.

### 10.3 What's available

- **JavaScript/TypeScript toolkit** — for websites, servers and apps
- **Command-line wallet** — for signing safely on a computer that's kept offline
- **Browser extension wallet** — keys stay out of websites entirely
- **Web dashboard** — trading, tokens, certificates, blockchain explorer
- **Full REST API** with a complete OpenAPI specification
- **Worked example app** you can run and read

### 10.4 Built to prevent mistakes

The toolkit builds every message from the same numbers it sends. You physically
cannot approve one amount and send a different one — a mistake that has cost
people real money on other platforms.

Amounts are handled in a way that can't lose precision on large numbers, which
is a genuine source of bugs elsewhere.

### 10.5 No smart contracts — on purpose

Hikmalayer has no programming layer for user-written contracts. That's a
deliberate choice with a clear reason:

> **Most of the money ever stolen from blockchains was stolen through bugs in
> smart contracts, not bugs in blockchains.**

By not having them, that entire category of theft simply doesn't exist here. The
cost is real too: you can't build arbitrary applications on Hikmalayer the way
you can on Ethereum. Tokens, trading, certificates and locked schedules are
built in and ready. Things beyond that aren't possible.

We say this openly rather than hiding it, because it's the right trade for a
chain built to hold certificates and value — and it's the wrong trade for
someone who wants to build an arbitrary app.

---

## 11. The people behind it

### 11.1 The company

**Bestower Labs Limited** develops and stewards Hikmalayer.

| | |
|---|---|
| Company | Bestower Labs Limited |
| Website | www.bestowerlabs.com |
| Role | Development, stewardship and governance of Hikmalayer |

### 11.2 The founder

**Muhammad Ayan Rao** — Founder and Director, Bestower Labs Limited.

Ayan Rao is the creator and lead architect of Hikmalayer. He is the author of
the Hikmalayer whitepaper and the Director and Person with Significant Control
of Bestower Labs Limited, the company that develops the protocol.

| | |
|---|---|
| Full name | Muhammad Ayan Rao |
| Known as | Ayan Rao |
| Role | Founder & Director, Bestower Labs Limited |
| Also | Creator and lead architect of Hikmalayer; author of the Hikmalayer whitepaper |
| Email | Ayanrao@bestowerlabs.com |
| Company website | www.bestowerlabs.com |


### 11.3 Suggested founder biography (short version)

*Once the fields above are filled in, this paragraph can be adjusted and used in
press kits, conference programmes and interview introductions.*

> Muhammad Ayan Rao is the founder of Bestower Labs Limited and the creator of
> Hikmalayer, a Layer 1 blockchain designed to remain secure in the age of
> quantum computing. He is the architect of Hikmalayer's dual-hybrid security
> model, which protects accounts with two independent types of cryptographic
> signature so that breaking either one is not enough to compromise them —
> making Hikmalayer one of a small number of blockchains to have implemented the
> NIST post-quantum signature standard across an entire protocol. He is also the
> author of the Hikmalayer whitepaper.

### 11.4 The founding idea, in his framing

*This is drawn from the project's own published documents and is safe to quote
as the project's position:*

> Every major blockchain in the world today publishes information that a
> sufficiently powerful quantum computer could turn into stolen funds. That data
> is public, permanent, and already being collected. The fix has to be built
> before the machine exists, not after — and it has to protect everything a key
> can do, not just the obvious parts.

### 11.5 Licensing and intellectual property

- **Source code:** HikmaLayer Business Source License 1.1
- **Contributions:** governed by the HikmaLayer Contributor License Agreement
- **Whitepaper:** Creative Commons Attribution 4.0 (free to share and translate,
  with attribution)
- **Copyright:** © 2026 Bestower Labs Limited

The whitepaper being freely shareable is deliberate — the ideas are meant to
travel.

---

## 12. Being honest: what it can't do yet

**Read this section before making any public claim about Hikmalayer.** Saying
these things openly builds far more trust than hiding them, and a competitor or
journalist will find them anyway.

### 12.1 It has not launched yet

Hikmalayer is built, tested and working, but the public network is not live. Do
not describe it as launched or trading.

### 12.2 It has not been independently audited

The security review so far has been done by the team that built it. They found
and fixed **13 security issues** and published every one of them — including two
serious problems in the quantum protection itself, found by reviewing their own
work.

But an outside expert review has **not** happened yet, and it is a firm
requirement before the network goes live with real value.

**Never say "audited" or "audit-ready" until an independent audit is complete.**

### 12.3 One part isn't quantum-proof yet

The system that picks who writes the next block still uses older maths. A
quantum computer could **predict** whose turn is coming — but it could **not**
steal coins, fake a block, or forge a signature, because those are protected
separately.

The reason it hasn't been upgraded is that the world's cryptographers haven't
finished agreeing on a standard for this particular piece yet. Hikmalayer is
waiting for the proper standard rather than inventing something and hoping.

Say this openly. "We fixed the parts that could lose people money, and we're
waiting for the standard on the remaining piece" is a *strong* answer, and it's
true.

### 12.4 Quantum-ready accounts are a choice, not automatic

Because they're slower and larger, users opt in. A user on a normal `hkm…`
account has the same exposure as any other blockchain.

### 12.5 Not on exchanges

HKM is not listed anywhere and there is no timeline. Never suggest otherwise.

Because Hikmalayer has no "bridge" to other blockchains — deliberately, since
bridges are the most-attacked part of the whole industry and have lost over a
billion dollars — an exchange wanting to list HKM must build a direct
integration. That's real work, and it's an honest barrier.

### 12.6 Not fully decentralised at launch

At launch, only approved participants can become validators. This is normal and
sensible for a new network, and it is planned to open up. But it means "not
decentralised yet" is the accurate description today.

### 12.7 No investment claims — ever

HKM is a network utility asset. It pays fees and secures the network.

- ❌ Never suggest HKM will rise in value
- ❌ Never present it as an investment or a return
- ❌ Never promise listings, partnerships or dates that aren't confirmed
- ❌ Never say it's a governance token — it carries no voting rights

Financial promotion is regulated in most countries. Take proper legal advice
before any campaign involving HKM.

### 12.8 What quantum protection does *not* cover

Two locks protect against someone *breaking the maths*. They don't protect
against:

- Malware on your computer
- Being tricked into approving something
- Losing your key (there is no recovery — that's true of every blockchain)

Be precise. Overclaiming here would be both wrong and easy to disprove.

---

## 13. Common questions

**Q: When will quantum computers actually break blockchain security?**
Nobody knows, and anyone giving a confident date is guessing. Estimates range
from several years to a couple of decades. The point is that data being
collected *today* can be attacked whenever that day arrives — so waiting isn't a
neutral choice.

**Q: Isn't this just marketing? Is it really quantum-proof?**
It's real and you can check it. Hikmalayer uses the official NIST standard
(FIPS 204, finalised 2024), the code is open to read, and the tests that prove
the protection works are in the repository. The tests assume the attacker
already has your normal key and show the account still holds.

**Q: Why keep the old lock at all if the new one is quantum-proof?**
Because the new one is young. It became a standard in 2024, and the old one has
survived decades of attack. If a weakness in the new one is ever found, the old
one is still holding the door. Using both means neither surprise is fatal.

**Q: Can I move my normal account to a quantum-ready one?**
Yes — you send your coins from one to the other. They're two separate accounts,
even though they come from the same key, so it's an ordinary transfer.

**Q: Do I need two passwords or two backups?**
No. One key, one backup. The second lock is created from the same secret you
already have.

**Q: What happens if I lose my key?**
Your coins are gone permanently. This is true of every blockchain. A recovery
system would be a back door, and back doors get used by the wrong people. Back
up your key.

**Q: Is my certificate visible on the blockchain?**
No. Only a digital fingerprint of it. The fingerprint can't be turned back into
the document, so nobody can read your details from the blockchain.

**Q: What if a certificate needs to be cancelled?**
The issuer cancels it on the blockchain, instantly. Every check from that moment
shows it as cancelled.

**Q: Can I connect Bitcoin or Ethereum to Hikmalayer?**
No, and that's deliberate. The technology for connecting blockchains — called a
bridge — is the most-attacked part of the industry, with over a billion dollars
stolen from bridges. Hikmalayer chose not to have one. The cost is that it's
harder to move assets in and out. The benefit is that the biggest target in
crypto simply doesn't exist here.

**Q: Can I build an app on Hikmalayer?**
You can build apps that *use* Hikmalayer — wallets, certificate systems,
trading tools, marketplaces — using the developer toolkit. You cannot write
programs that run *inside* Hikmalayer, because there's no smart contract layer.
That's a deliberate trade-off (see §10.5).

**Q: How is this different from Bitcoin or Ethereum?**
Bitcoin is digital money. Ethereum is a platform for running programs.
Hikmalayer is built for proving documents are genuine and moving digital assets
safely — with quantum protection built in and certificates as a native feature.

**Q: Is it environmentally damaging like Bitcoin?**
Much less so. On Bitcoin, thousands of machines worldwide race to solve the same
puzzle and all but one waste their effort. On Hikmalayer, **only the one chosen
validator does the work** — there's no race. The difficulty is also capped, so
the effort per block stays bounded. It isn't zero, and we won't claim it is.

**Q: Who controls Hikmalayer?**
The software is developed by Bestower Labs Limited. But the network's rules are
enforced by every computer running it, so a change nobody adopts simply doesn't
happen. There is currently no on-chain voting system.

---

## 14. Ready-to-use copy

Copy and paste these directly.

### Twitter / X bio (160 characters)

> The Layer 1 blockchain built for the quantum era. Two independent locks on
> every account. Verify certificates without exposing them. 🔐

### One-liner

> Hikmalayer is a blockchain built to survive quantum computers — and to prove
> certificates are genuine without ever putting your documents online.

### Short description (50 words)

> Hikmalayer is a Layer 1 blockchain designed for the quantum era. Accounts can
> be protected by two independent types of signature, so breaking one isn't
> enough to steal from them. It also lets institutions prove certificates are
> genuine — publishing only a fingerprint, never the document itself.

### Medium description (100 words)

> Hikmalayer is a Layer 1 blockchain built from scratch for a world with quantum
> computers. Every blockchain today relies on maths that a powerful enough
> quantum computer could break — and the data needed to attack them is already
> public and already being collected. Hikmalayer's accounts can be protected by
> two completely different types of signature at once, so an attacker has to
> break both. It also has certificate verification, its own token standard and a
> decentralised exchange built directly into the blockchain — no add-on programs,
> and therefore no add-on program bugs.

### Long description (250 words)

> Hikmalayer is a Layer 1 blockchain created by Bestower Labs Limited and
> designed for a problem the industry has been slow to face: quantum computers.
>
> Every major blockchain protects your funds with a type of digital signature
> that a sufficiently powerful quantum computer could break. Worse, the
> information needed to carry out that attack is already public and can be saved
> today and used years later. It is not a problem that can be fixed afterwards.
>
> Hikmalayer's answer is a dual-hybrid design. Accounts can be protected by two
> independent kinds of signature — the standard one used across the industry, and
> ML-DSA-65, the post-quantum standard finalised by NIST in 2024. Both are
> required. An attacker must break both to take anything, and the protection
> covers not just transfers but staking, withdrawals and block production.
>
> Beyond security, Hikmalayer is built for verifying documents. Universities,
> professional bodies and employers can prove a certificate is genuine by
> publishing only a digital fingerprint — never the document, never the personal
> details. Verification takes seconds and requires trusting nobody. Revocation is
> instant and permanent.
>
> It also includes a native token standard where no token can hide a secret
> ability to create more coins or freeze wallets, and a decentralised exchange
> built into the blockchain itself rather than added on top.
>
> Hikmalayer has not yet launched publicly and has not yet completed an
> independent security audit — both of which are required before it goes live.

### Press release opening

> **Bestower Labs Limited unveils Hikmalayer, a Layer 1 blockchain built for the
> post-quantum era**
>
> Bestower Labs Limited today announced Hikmalayer, a Layer 1 blockchain designed
> to remain secure against quantum computing — a threat that experts warn could
> render current blockchain cryptography obsolete.
>
> Unlike existing networks, Hikmalayer accounts can be secured by two independent
> types of cryptographic signature at once: the elliptic-curve signatures used
> across the industry today, and ML-DSA-65, the post-quantum standard finalised
> by the US National Institute of Standards and Technology in 2024. Both are
> required to authorise a transaction, so compromising either one alone is not
> enough.
>
> "The information needed to attack today's blockchains is already public and
> already being collected," said Muhammad Ayan Rao, Founder and Director of
> Bestower Labs Limited. "That's why this has to be built before the machine
> exists, not after."
>
> [Continue with launch details, availability and quotes.]

### Elevator pitch (30 seconds, spoken)

> "Every blockchain today — Bitcoin, Ethereum, all of them — is protected by
> maths that quantum computers will eventually break. And here's the catch:
> attackers can copy the data now and crack it later, so waiting isn't safe.
>
> Hikmalayer puts two completely different locks on your account. A quantum
> computer can pick the first one. It can't pick the second. So your money stays
> put.
>
> It also lets universities and employers verify certificates instantly, without
> the certificate itself ever going online.
>
> It's built, it's tested, and it's getting an independent security audit before
> it launches."

---

## 15. Facts and figures

Every number here is verifiable in the public code and documentation.

### Technology

| | |
|---|---|
| Type | Layer 1 blockchain (its own base network) |
| Built in | Rust |
| Consensus | Hybrid: Proof of Stake picks, Proof of Work confirms |
| Post-quantum standard | ML-DSA-65 (NIST FIPS 204, finalised 2024) |
| Classical signatures | secp256k1 ECDSA |
| Hashing | SHA-256 |
| Target block time | 15 seconds |
| Address formats | `hkm…` (standard) and `hkq…` (quantum-ready) |
| Smart contracts | None — by design |
| Bridges to other chains | None — by design |

### Economics

| | |
|---|---|
| Coin | HKM |
| Total supply | ~100 billion HKM |
| At launch | 30 billion (30%) |
| Earned over time | ~70 billion (70%) |
| Decimal places | 6 |
| Starting block reward | 3,700 HKM |
| Halving period | 9,500,000 blocks (~4.5 years) |
| Permanent reward | 50 HKM per block |
| Minimum to become a validator | 10,000 HKM |
| Exchange fee | 0.30% to liquidity providers |

### Testing

| | |
|---|---|
| Core tests | 139 |
| Attacker tests | 40 |
| Wallet compatibility tests | 58 |
| Live network tests | 21 |
| Security issues found and fixed | 13 (all published) |
| Independent audit | **Not yet done — required before launch** |

### Performance

| | |
|---|---|
| Measured throughput | 14.88 transactions per second (local multi-node test) |
| Average response time | ~67 milliseconds |
| Memory per node | ~4–5 MB |
| Signing time (standard) | Under 1 millisecond |
| Signing time (quantum-ready) | ~11 milliseconds |

*Note: performance was measured on a single machine running several nodes. It
does not represent a worldwide network — that measurement needs a public test
network, which is planned before launch.*

---

## 16. Words to use and avoid

### ✅ Safe to say

- "Built for the quantum era" / "quantum-ready"
- "Uses the official NIST post-quantum standard"
- "Two independent signatures — breaking one isn't enough"
- "Verify certificates without publishing them"
- "No smart contracts, so no smart-contract bugs"
- "Powerful computers alone can't attack it"
- "Open source and independently verifiable"
- "Pre-launch, with an independent audit planned before going live"

### ❌ Do not say

| Don't say | Why | Say instead |
|---|---|---|
| "Unhackable" / "100% secure" | Nothing is, and it invites a challenge | "Designed to withstand quantum attacks" |
| "Audited" | No independent audit has happened | "Security-reviewed, with an independent audit planned" |
| "Quantum-proof" (unqualified) | One component isn't yet | "Quantum-ready across all value operations" |
| "Coming to exchanges" | Nothing is confirmed | Don't mention listings at all |
| "Investment" / "returns" / "profit" | Regulated financial promotion | "Network utility asset for fees and staking" |
| "Fully decentralised" | Launch is permissioned | "Decentralisation is planned as the network matures" |
| "Faster than Ethereum/Solana" | Not measured on a real network | "Designed for reliability; performance measured before launch" |
| "Governance token" | HKM carries no voting rights | "Utility asset for fees and network security" |

### The tone that works

Hikmalayer's whole credibility rests on being straight with people — the
project's own documents openly list its own security failures and unfinished
work. Marketing should sound like the same organisation.

**Confident about what's built. Honest about what isn't. Never hyped.**

That combination is rarer than it should be in this industry, and it's a
genuine advantage. Use it.

---

## 17. Contact

| | |
|---|---|
| Company | Bestower Labs Limited |
| Founder & Director | Muhammad Ayan Rao |
| Email | Ayanrao@bestowerlabs.com |
| Website | www.bestowerlabs.com |

### Where to find more

| Audience | Document |
|---|---|
| General public | This document |
| Technical readers | `docs/Whitepaper.md` — the full technical paper |
| Quick technical overview | `docs/whitepaper_short_version.md` |
| Anyone asking about quantum | `docs/quantum_readiness.md` |
| Security researchers | `docs/security_assessment.md` — all 13 issues, published |
| Developers | `README.md` and `sdk/README.md` |
| Questions about tokens and exchanges | `docs/hts_and_listings.md` |

---

*This document is for public information only. It is not an offer to sell, a
solicitation to buy, investment advice, or a promise of future performance. HKM
is a network utility asset. Cryptocurrency and blockchain projects carry risk,
including total loss. Take independent professional advice before any financial
decision. Hikmalayer has not launched publicly and has not completed an
independent security audit.*

*© 2026 Bestower Labs Limited*
