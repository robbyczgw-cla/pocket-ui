# The image's grub.cfg checks that need more than a grep (scripts/image/check.sh sources this file; its test is
# scripts/image/test-grub-checks.sh). Sourced, not run.

# Whether the GRUB config $1 prints nothing and leaves fbcon alone: no `echo` command and no `fbcon=` word on any
# line that is not a comment. The comment in deploy/sd-efi/grub.cfg names fbcon=rotate and fbcon=map to warn
# against them; that is no setting.
grub_cfg_quiet_clean() {
  ! grep -Ev '^[[:space:]]*#' "$1" | grep -Eq '^[[:space:]]*echo[[:space:]]|fbcon='
}

# Whether deploy/grub-quiet.sh ($2), the script behind scripts/boot-quiet.sh, finds the GRUB config $1 already
# quiet: `status` says "quiet start: on" and `apply` changes nothing. A copy in a fake /flash, so $1 stays as it is.
grub_cfg_quiet_for_boot_quiet() {
  gq_tmp=$(mktemp -d) || return 1
  mkdir -p "$gq_tmp/boot/grub"
  cp "$1" "$gq_tmp/boot/grub/grub.cfg"
  gq_ok=0
  if POCKETUI_FLASH=$gq_tmp sh "$2" status 2>&1 | grep -qx 'quiet start: on' &&
    POCKETUI_FLASH=$gq_tmp sh "$2" apply 2>&1 | grep -q 'already on; nothing changed' &&
    cmp -s "$1" "$gq_tmp/boot/grub/grub.cfg" && [ ! -e "$gq_tmp/boot/grub/grub.cfg.before-quiet" ]; then
    gq_ok=1
  fi
  rm -rf "$gq_tmp"
  [ $gq_ok = 1 ]
}

# Whether the Boot Loader Specification entry $1 (deploy/sd-efi/loader/entries/pocket-ui.conf, read by OpenABL's
# "Boot mode: Linux") starts the same kernel, device tree and command line as the "pocket-ui" entry of the GRUB
# config $2 (started by Retroid's u-boot or any UEFI firmware), and has nothing else that boots: exactly the keys
# title, linux, devicetree, options, once each, and no initrd (GRUB loads none).
bls_matches_grub() {
  bm_g=$(awk '/^menuentry "pocket-ui"/ { e = 1 } e && /^}/ { exit } e' "$2")
  bm_glinux=$(printf '%s\n' "$bm_g" | awk '$1 == "linux" { print $2 }')
  bm_gopts=$(printf '%s\n' "$bm_g" | sed -n 's/^[[:space:]]*linux [^ ]* //p')
  bm_gdtb=$(printf '%s\n' "$bm_g" | awk '$1 == "devicetree" { print $2 }')
  [ -n "$bm_glinux" ] && [ -n "$bm_gdtb" ] && [ -n "$bm_gopts" ] || return 1
  [ "$(awk 'NF { print $1 }' "$1" | sort | tr '\n' ' ')" = "devicetree linux options title " ] || return 1
  [ "$(awk '$1 == "linux" { print $2 }' "$1")" = "$bm_glinux" ] &&
    [ "$(awk '$1 == "devicetree" { print $2 }' "$1")" = "$bm_gdtb" ] &&
    [ "$(sed -n 's/^options[[:space:]]*//p' "$1")" = "$bm_gopts" ]
}
