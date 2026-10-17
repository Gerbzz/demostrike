# Second public CS2 oracle fixture

The pinned CSDM analyzer comparison also uses the `de_ancient` Valve Premier
recording published by Peter Xenopoulos on Figshare:

- Title: `match730_003736456444682174484_1173793269_201`
- DOI: https://doi.org/10.6084/m9.figshare.28440473.v1
- File: https://api.figshare.com/v2/file/download/52456259
- License: CC BY 4.0, as documented by the public fixture maintainer in
  `taua-almeida/cs2-analyser-tool` at commit
  `eca5c9da13534d5d42e5dd54c7535399f2c64489`,
  `analysis/testdata/README.md`.
- SHA-256: `b29a9cb537a181deef97b15cfed10ee722a37999644a27bb2226fdd77a1337fc`

Hosted CI downloads at most 350 MB into the runner temporary directory,
checks this exact hash and the PBDEMS2 header, and never commits or uploads
the raw demo. The generated analyzer facts and comparison report are retained
as a short-lived workflow artifact. Attribution and license apply to the
recording; no local executable or recording download is used.
