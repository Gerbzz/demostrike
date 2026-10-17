# Public CS2 decoder fixture

The native decoder probe downloads `src/parser/test_demo.dem` from
[`LaihoE/demoparser` at commit `45ca85ae`](https://github.com/LaihoE/demoparser/blob/45ca85aeac8fb0de9d385124d2c7fe0e7b8ff0c8/src/parser/test_demo.dem).
Its maintainer added the file in [commit `4131a4f`](https://github.com/LaihoE/demoparser/commit/4131a4fc02fda291b22421c20e1ca33f149535a7),
and the public repository has an [MIT license](https://github.com/LaihoE/demoparser/blob/45ca85aeac8fb0de9d385124d2c7fe0e7b8ff0c8/LICENSE).
The test downloads the fixture transiently on a hosted runner; DemoStrike does
not redistribute the binary. Its size is exactly 60,601,900 bytes and its Git
blob ID is `e5cc97203262ab36bb756514b02d56eb6bd972a0`. The workflow checks
both values and the CS2 `PBDEMS2` magic before decoding. No private demo is
uploaded or read by CI.

The larger `osztenkurden/cs2parser` public release fixture is 329,136,562
bytes, so this smaller repository fixture keeps the hosted download bounded.
The probe is intended to reveal whether DemoStrike's actual `source2-demo`
decoder accepts a public real-game recording; a passing header check alone
does not count as parser acceptance.
