{ fetchFromGitHub
, runCommand
}:
let
  src = fetchFromGitHub {
    owner = "omacom";
    repo = "omarchy";
    # quattro / v4.0.3
    rev = "0534987009061cbe2dacdde4ad564092ab698d12";
    sha256 = "sha256-+LF1Etj6akqmam9stdTeJJNBfoxAL+ZYyWvqo9R2P2E=";
  };
in
runCommand "oath-omarchy-pack" { } ''
  mkdir -p $out
  for d in bin shell default themes config applications migrations install; do
    if [ -d ${src}/$d ]; then
      cp -a ${src}/$d $out/$d
    fi
  done
  for f in version LICENSE README.md icon.png logo.svg; do
    if [ -e ${src}/$f ]; then
      cp -a ${src}/$f $out/$f
    fi
  done
  chmod -R u+rwX $out
  find $out/bin -type f -exec chmod 755 {} \; 2>/dev/null || true
  echo ${src.rev} > $out/.oath-rev
''
