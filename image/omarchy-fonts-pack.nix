{ nerd-fonts
, liberation_ttf
, noto-fonts-color-emoji
, yaru-theme
, runCommand
}:
# Desk fonts + Yaru XCursor for the Omarchy session. Arch ships these as
# ttf-jetbrains-mono-nerd-basic, ttf-liberation, noto-fonts-emoji, yaru-icon-theme.
runCommand "oath-omarchy-fonts"
  {
    NF = nerd-fonts.jetbrains-mono;
    LIB = liberation_ttf;
    EMO = noto-fonts-color-emoji;
    YARU = yaru-theme;
  } ''
  mkdir -p $out/share/fonts $out/share/icons/Yaru
  # Ligature JetBrainsMono Nerd Font — family name Omarchy aliases as monospace.
  for f in Regular Bold Italic BoldItalic; do
    src="$NF/share/fonts/truetype/NerdFonts/JetBrainsMono/JetBrainsMonoNerdFont-$f.ttf"
    if [ -f "$src" ]; then cp -a "$src" $out/share/fonts/; fi
  done
  cp -a "$LIB"/share/fonts/truetype/*.ttf $out/share/fonts/
  cp -a "$EMO"/share/fonts/noto/NotoColorEmoji.ttf $out/share/fonts/
  cp -a "$YARU"/share/icons/Yaru/cursors $out/share/icons/Yaru/
  cat >$out/share/icons/Yaru/cursor.theme <<'EOF'
[Icon Theme]
Name=Yaru
Comment=Ubuntu Yaru cursors
EOF
  cat >$out/share/icons/Yaru/index.theme <<'EOF'
[Icon Theme]
Name=Yaru
Comment=Ubuntu Yaru cursors
Inherits=hicolor
EOF
  mkdir -p $out/share/icons/default
  cat >$out/share/icons/default/index.theme <<'EOF'
[Icon Theme]
Name=Default
Comment=Default Cursor Theme
Inherits=Yaru
EOF
''
