# VCDIFF adapter canaries

These are minimal project-owned synthetic fixtures copied verbatim from `vcdiff-rs` commit
`aca4282161ba607a307e76a48ccb7eec794a650b`. They exercise Overseer's path adapter rather than
duplicating the dependency's codec conformance suite.

## Provenance

The ID-1 anchors were produced by xdelta 3.2.0 at release commit
`ff322e592383227b0d65ddfde7e0e5bbc504dc15`. Its executable SHA-256 was
`53d90226615f217d3380c39892833311b4e24acd863e1ca01f14b5e772e2e6d0`, its configuration enabled
`SECONDARY_DJW=1`, and `XDELTA` was unset.

```text
one = bytes(65 + ((i * 17 + i // 7) % 12) for i in range(512))
xdelta3.exe -f -e -A -N -n -S djw1 one.raw one.vcdiff
xdelta3.exe -f -e -A -N -n -S none one.raw one-none.vcdiff
xdelta3.exe -f -d one.vcdiff one.decoded
```

The DATA section was extracted from each delta. The decoded-size varint was removed from the DJW
form to produce the payload anchor, and the matching DATA bytes from the uncompressed form produced
the raw anchor. The producer decoded the complete ID-1 delta back to the generated target.

The ID-2 fixture and target were produced by the same xdelta 3.2.0 executable:

```text
xdelta3.exe -e -a -A -N -S lzma -W 16384 -s source.bin target.bin xdelta-3.2.0-lzma.vcdiff
```

The delta has six 16 KiB target windows and exercises persistent ID-2 section state. It does not
reference source bytes, so the adapter canary intentionally creates an empty source file.

## Identities

| File | Bytes | SHA-256 |
|---|---:|---|
| `djw-one-xdelta-3.2.0.payload.bin` | 197 | `a22ce0c94a213f864725d794ccf065d7c0c960ec46fdb57ab9ae407fa6b1a539` |
| `djw-one-xdelta-3.2.0.raw.bin` | 512 | `5367055fb2ff891de9954be88ca51593bdeeac3aa59e8cc039b9f5ae424e29e6` |
| `xdelta-3.2.0-lzma.vcdiff` | 16714 | `f40d8e39994dfd7460cf63883764159cd4fae8285d3cc8c4f8ef231a969f007c` |
| `target.bin` | 98304 | `c21ff467100a57e3495cf97bd025a9c903c32a85fd927f5d13b559d2b197daae` |
