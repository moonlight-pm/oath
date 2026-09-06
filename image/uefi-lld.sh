#!/bin/sh
# rustc UEFI linker on Oath. rust-lld is a glibc ELF; there is no /lib64.
interp=/oath/store/pkg/glibc/lib/ld-linux-x86-64.so.2
glibc=/oath/store/pkg/glibc/lib
rlib=/oath/store/pkg/rustc/lib
zlib=/oath/store/pkg/git/lib
real=/oath/store/pkg/rustc/lib/rustlib/x86_64-unknown-linux-gnu/bin/rust-lld
exec "$interp" --library-path "$rlib:$glibc:$zlib" "$real" "$@"
