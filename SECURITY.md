# Security

## Reporting a vulnerability

Email **hello@keyvotion.com** with "Kineto Runtime security" in the subject.
Please do not open a public issue for a vulnerability. Include the affected
commit, platform, and a `.kvk` or steps that reproduce it, if you have them.

You should get an acknowledgement within a few working days. There is no bug
bounty.

## Scope

The runtime parses `.kvk` documents, which may come from untrusted sources.
In scope:

- memory safety or crashes in `engine/kineto_engine` when loading or playing a
  document (including through the C ABI, JNI and wasm bindings);
- unbounded resource use triggered by a document;
- issues in the Swift, Kotlin, Dart and web wrappers.

Out of scope: the Kineto editor app and website, which are not in this
repository.

## Supported versions

No release has been published. Fixes land on `main`.
