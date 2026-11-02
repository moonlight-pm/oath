# certdata.txt, Mozilla's root store, to a PEM file. Takes the roots trusted to
# identify servers, less those with a distrust-after date and those distrust.txt
# names. Usage: certdata2pem.py certdata.txt distrust.txt out.pem
import base64, hashlib, re, sys

text = open(sys.argv[1], encoding="utf8").read()
distrust = set()
for line in open(sys.argv[2]):
    if line.strip() and not line.startswith("#"):
        fp = line.split()[0]
        if not re.fullmatch("[0-9a-f]{64}", fp):
            sys.exit(f"distrust.txt: not a fingerprint: {fp}")
        distrust.add(fp)


def octal(block, field):
    m = re.search(field + r" MULTILINE_OCTAL\n(.*?)\nEND", block, re.S)
    return bytes(int(x, 8) for x in re.findall(r"\\(\d{3})", m[1])) if m else None


certs, trust = [], {}
for block in re.split(r"\n(?=CKA_CLASS CK_OBJECT_CLASS )", text):
    label = re.search(r'CKA_LABEL UTF8 "(.*)"', block)
    if not label:
        continue
    if "CKO_CERTIFICATE" in block.split("\n", 1)[0]:
        certs.append((label[1], octal(block, "CKA_VALUE"), octal(block, "CKA_NSS_SERVER_DISTRUST_AFTER")))
    elif "CKO_NSS_TRUST" in block.split("\n", 1)[0]:
        trust[(label[1], octal(block, "CKA_ISSUER"), octal(block, "CKA_SERIAL_NUMBER"))] = (
            "CKA_TRUST_SERVER_AUTH CK_TRUST CKT_NSS_TRUSTED_DELEGATOR" in block
        )

servers = {label for (label, _, _), yes in trust.items() if yes}
if len(certs) < 100:
    sys.exit(f"certdata.txt: only {len(certs)} roots: a broken parse")
seen, kept = set(), 0
with open(sys.argv[3], "w") as out:
    out.write("## Oath's CA roots (pkg:certs): Mozilla's for servers, less pkgs/certs/distrust.txt.\n\n")
    for label, der, after in certs:
        if label not in servers:
            continue
        fp = hashlib.sha256(der).hexdigest()
        if fp in distrust:
            seen.add(fp)
            print(f"== distrusted: {label}", file=sys.stderr)
            continue
        if after:
            print(f"== Mozilla distrusts what it issued after {after.decode()}: {label}", file=sys.stderr)
            continue
        kept += 1
        b64 = base64.b64encode(der).decode()
        body = "\n".join(b64[i : i + 64] for i in range(0, len(b64), 64))
        out.write(f"{label}\n{'=' * len(label)}\n# SHA-256: {fp}\n")
        out.write(f"-----BEGIN CERTIFICATE-----\n{body}\n-----END CERTIFICATE-----\n\n")
for fp in sorted(distrust - seen):
    print(f"== already gone from upstream: {fp}", file=sys.stderr)
if kept < 90:
    sys.exit(f"only {kept} roots: a broken parse")
print(f"== {kept} roots", file=sys.stderr)
