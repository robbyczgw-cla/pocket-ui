# The image check's "no BIOS files" (scripts/image/check-h700.sh and check.sh source this file; its test is
# scripts/image/test-bios-checks.sh). Sourced, not run.
# Whether the unpacked STORAGE $1 and the FAT partition's file list $2 hold no BIOS file: no name with "bios" in it.
# One exception, at exactly one place: the notice of DSperate's built-in open FreeBIOS (BSD-2-Clause; the source's
# src/core/bios/LICENSE.freebios), which scripts/dsperate-files.py puts in the release $3 as
# pocket-ui/releases/$3/cores/dsperate/licences/DSperate_src_core_bios/LICENSE.freebios. It is a licence text, and
# ds-checks.py has already compared it with the release's sums. Anything else in that folder, or the same name elsewhere,
# still counts.
no_bios_files() {
  _nb_lic=$1/pocket-ui/releases/$3/cores/dsperate/licences/DSperate_src_core_bios
  [ -z "$(find "$1" -iname '*bios*' ! -path "$_nb_lic" ! -path "$_nb_lic/LICENSE.freebios")" ] && ! grep -qi bios "$2"
}
