# Each locale as a directory (no archive). localedef exits 1 when it
# only warned.
export I18NPATH=/oath/store/pkg/glibc/live/share/i18n
mkdir -p /out/lib/locale
for l in C en_US; do
	localedef --no-archive -i "$l" -f UTF-8 "/out/lib/locale/$l.utf8" || [ $? -eq 1 ]
done
ls /out/lib/locale/C.utf8/LC_CTYPE /out/lib/locale/en_US.utf8/LC_CTYPE
