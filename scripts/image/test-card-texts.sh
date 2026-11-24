#!/bin/sh
# Test of scripts/image/card-texts.sh: every card's SOURCE-OFFER.txt, README.txt and grub.cfg name that card's model and
# image, and no other model of its family. Renders the templates the way the builders do, for every model of every family,
# and feeds the image checks' readings with them; the 0.6.0 way (only the tree's file name put in) must be refused.
# Needs no Docker and no image. Run by scripts/image/test-h700-pins.sh (the gate).
set -u
cd "$(dirname "$0")/../.." || exit 1
ROOT=$PWD
. scripts/image/pins.sh
. scripts/image/tools.sh
. scripts/image/grub-checks.sh
. scripts/image/card-texts.sh
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
fails=0
V=9.8.7
check() {
  if eval "$2"; then
    echo "ok   $1"
  else
    echo "FAIL $1"
    fails=$((fails + 1))
  fi
}
# The placeholders with stand-in values, as scripts/make-image.sh and make-image-family.sh fill them in.
fill() {
  sed -e "s|@VERSION@|$V|g" -e "s|@ISSUES_URL@|https://github.com/o/pocket-ui/issues|g" -e "s|@RELEASE_URL@|https://github.com/o/pocket-ui/releases/tag/v$V|g" \
    -e "s|@MODEL_NAME@|$1|g" -e "s|@IMAGE_NAME@|$2|g" -e "s|@DTB_NAME@|$3|g" -e "s|@EFI_DTB_FILE@|$3|g" -e "s|@RAM@|DDR4|g" -e 's|@[A-Z_]*@|x|g'
}

echo "== SM8250 (scripts/make-image.sh, scripts/image/check.sh)"
for m in rp-mini-v2 rp5 flip2; do
  sm8250_pins "$m"
  D=$T/$m
  mkdir -p "$D"
  sm8250_card_text <deploy/sd-efi/grub.cfg >"$D/grub.cfg"
  sm8250_card_text <deploy/sd-efi/loader/entries/pocket-ui.conf >"$D/bls.conf"
  fill "" "" "" <deploy/image/SOURCE-OFFER.txt.in | sm8250_card_text >"$D/SOURCE-OFFER.txt"
  fill "" "" "" <deploy/image/README.txt.in | sm8250_card_text >"$D/README.txt"
  # The builder's own notes step for the Pocket 5 and Flip 2 (the Python inside scripts/make-image.sh).
  if [ "$m" != rp-mini-v2 ]; then
    sed -n "/<<'PYNOTES'\$/,/^PYNOTES\$/p" scripts/make-image.sh | sed '1d;$d' >"$T/notes.py"
    python3 -I "$T/notes.py" "$D/README.txt" "$SM8250_MODEL_NAME" "$SM8250_DTB_FILE" "$m"
  fi
  check "$m: SOURCE-OFFER.txt names $SM8250_MODEL_NAME and pocket-ui-$V-$m.img.xz" 'offer_names_card "$D/SOURCE-OFFER.txt" "$V" "$SM8250_MODEL_NAME" "pocket-ui-$V-$m"'
  check "$m: grub.cfg's comments name $SM8250_MODEL_NAME and $SM8250_DTB_FILE" 'grub_names_card "$D/grub.cfg" "$SM8250_MODEL_NAME" "$SM8250_DTB_FILE"'
  check "$m: README.txt names $SM8250_MODEL_NAME and the image to write" 'grep -q "for the $SM8250_MODEL_NAME: SD card image" "$D/README.txt" && grep -qF "Write pocket-ui-$V-$m.img.xz " "$D/README.txt"'
  for f in SOURCE-OFFER.txt README.txt grub.cfg bls.conf; do
    check "$m: $f names no other SM8250 model" 'card_names_no_other "$D/$f" SM8250 "$m"'
  done
  check "$m: the BLS entry starts what grub.cfg starts (kernel, $SM8250_DTB_FILE, command line)" 'bls_matches_grub "$D/bls.conf" "$D/grub.cfg"'
  check "$m: grub.cfg is quiet and loads only $SM8250_DTB_FILE" 'grub_cfg_quiet_clean "$D/grub.cfg" && [ "$(grep -c "^ *devicetree /boot/pocket/$SM8250_DTB_FILE\$" "$D/grub.cfg")" = 1 ]'
  [ "$m" != rp-mini-v2 ] || check "rp-mini-v2: the filter changes nothing on the Mini V2's grub.cfg and BLS entry" 'cmp -s "$D/grub.cfg" deploy/sd-efi/grub.cfg && cmp -s "$D/bls.conf" deploy/sd-efi/loader/entries/pocket-ui.conf'
  # 0.6.0: only the tree's file name was put in. The Pocket 5 and Flip 2 cards said "Retroid Pocket Mini V2".
  if [ "$m" != rp-mini-v2 ]; then
    sed "s/miniv2-noufs.dtb/$SM8250_DTB_FILE/g" deploy/sd-efi/grub.cfg >"$D/grub-060.cfg"
    fill "" "" "" <deploy/image/SOURCE-OFFER.txt.in >"$D/offer-060.txt"
    check "$m: 0.6.0's grub.cfg (only the tree put in) is refused" '! grub_names_card "$D/grub-060.cfg" "$SM8250_MODEL_NAME" "$SM8250_DTB_FILE" && ! card_names_no_other "$D/grub-060.cfg" SM8250 "$m" >/dev/null'
    check "$m: 0.6.0's SOURCE-OFFER.txt (the Mini V2's) is refused" '! offer_names_card "$D/offer-060.txt" "$V" "$SM8250_MODEL_NAME" "pocket-ui-$V-$m" && ! card_names_no_other "$D/offer-060.txt" SM8250 "$m" >/dev/null'
  fi
done

echo "== families (scripts/make-image-family.sh, scripts/image/check-h700.sh)"
for fam in H700 SM8550 SM4450 RK3566 RK3326; do
  eval "list=\${${fam}_MODELS:-}"
  [ -n "$list" ] || continue
  f=$(echo "$fam" | tr 'A-Z' 'a-z')
  for m in $(echo "$list" | awk 'NF { print $1 }'); do
    name=$(card_models "$fam" | awk -F '|' -v m="$m" '$1 == m { print $2 }')
    dtb=$(echo "$list" | awk -v m="$m" '$1 == m { print $2 }')
    efi=0; ! is_efi_card "$fam" "$m" || efi=1
    gen=0; ! is_generic "$fam" "$m" || gen=1
    dev=0; ! is_devtest "$fam" "$m" || dev=1
    games=0; ! has_games_drive "$fam" "$m" || games=1
    D=$T/$fam-$m
    mkdir -p "$D"
    for t in README SOURCE-OFFER; do
      fill "$name" "pocket-ui-$V-$m" "$dtb" <"deploy/image/$t-$f.txt.in" | notes_blocks "$games" "$efi" "$gen" "$dev" >"$D/$t.txt"
    done
    check "$fam $m: SOURCE-OFFER.txt names $name and pocket-ui-$V-$m.img.xz, no other model" \
      'offer_names_card "$D/SOURCE-OFFER.txt" "$V" "$name" "pocket-ui-$V-$m" && card_names_no_other "$D/SOURCE-OFFER.txt" "$fam" "$m"'
    check "$fam $m: README.txt names $name and no other model" 'grep -q "for the $name: SD card image" "$D/README.txt" && card_names_no_other "$D/README.txt" "$fam" "$m"'
    if [ $efi = 1 ]; then
      sm8550_efi_pins "$m"
      sm8550_efi_card_text <deploy/sd-efi-sm8550/grub.cfg >"$D/grub.cfg"
      sm8550_efi_card_text <deploy/sd-efi-sm8550/loader/entries/pocket-ui.conf >"$D/bls.conf"
      check "$fam $m: grub.cfg names $EFI_MODEL_NAME and $EFI_DTB_FILE, no other model" 'grub_names_card "$D/grub.cfg" "$EFI_MODEL_NAME" "$EFI_DTB_FILE" && card_names_no_other "$D/grub.cfg" "$fam" "$m"'
      check "$fam $m: the BLS entry starts what grub.cfg starts" 'bls_matches_grub "$D/bls.conf" "$D/grub.cfg"'
      if [ "$m" != rp6 ]; then
        sed "s/rp6-noufs.dtb/$EFI_DTB_FILE/g" deploy/sd-efi-sm8550/grub.cfg >"$D/grub-060.cfg"
        check "$fam $m: the Pocket 6's grub.cfg with only the tree put in is refused" '! grub_names_card "$D/grub-060.cfg" "$EFI_MODEL_NAME" "$EFI_DTB_FILE"'
      fi
    fi
  done
done

echo "== card_names_no_other"
printf 'pocket-ui on the Retroid Pocket 6 TOP-DPAD\n' >"$T/topdpad.txt"
check "the TOP-DPAD's own name is not the Pocket 6's" 'card_names_no_other "$T/topdpad.txt" SM8550 rp6-top-dpad'
check "on the Pocket 6's card the TOP-DPAD's name is another model" '! card_names_no_other "$T/topdpad.txt" SM8550 rp6 >/dev/null'
printf 'the Retroid Pocket 6 TOP-DPAD, the Retroid Pocket 6\n' >"$T/both.txt"
check "on the TOP-DPAD's card the Pocket 6's name outside its own is another model" '! card_names_no_other "$T/both.txt" SM8550 rp6-top-dpad >/dev/null'
printf 'write pocket-ui-1-rp6.img.xz\n' >"$T/img.txt"
check "another model's image name is refused" '! card_names_no_other "$T/img.txt" SM8550 nova >/dev/null'
printf 'the Retroid Pocket Flip2\n' >"$T/flip2.txt"
check "the Flip 2's own name holds its short name" 'card_names_no_other "$T/flip2.txt" SM8250 flip2'
printf 'the Mini V2 device tree\n' >"$T/short.txt"
check "the Mini V2's short name on the Pocket 5's card is refused" '! card_names_no_other "$T/short.txt" SM8250 rp5 >/dev/null'
check "a model not in the family is refused" '! card_names_no_other "$T/short.txt" SM8250 rp6 >/dev/null'

echo "== the builders and checks use it"
check "make-image.sh puts this card's texts into grub.cfg, the BLS entry, README and SOURCE-OFFER" \
  'grep -qF "sm8250_card_text <\"\$ROOT/deploy/sd-efi/grub.cfg\"" scripts/make-image.sh && grep -qF "sm8250_card_text <\"\$ROOT/deploy/sd-efi/loader/entries/pocket-ui.conf\"" scripts/make-image.sh && grep -qF "\"\$ROOT/deploy/image/\$f.txt.in\" | sm8250_card_text >" scripts/make-image.sh'
check "make-image.sh no longer puts in only the tree's name" '! grep -q "sed \"s/miniv2-noufs.dtb/" scripts/make-image.sh'
check "make-image-family.sh puts the SM8550 Retroid's texts into grub.cfg and the BLS entry" \
  'grep -qF "sm8550_efi_card_text <\"\$ROOT/deploy/sd-efi-sm8550/grub.cfg\"" scripts/make-image-family.sh && grep -qF "sm8550_efi_card_text <\"\$ROOT/deploy/sd-efi-sm8550/loader/entries/pocket-ui.conf\"" scripts/make-image-family.sh && ! grep -q "sed \"s/rp6-noufs.dtb/" scripts/make-image-family.sh'
check "check.sh reads grub.cfg, README and SOURCE-OFFER for this card's model" \
  '[ "$(grep -c "card_names_no_other" scripts/image/check.sh)" = 3 ] && grep -q "grub_names_card" scripts/image/check.sh && grep -q "offer_names_card" scripts/image/check.sh'
check "check-h700.sh reads grub.cfg, README and SOURCE-OFFER for this card's model" \
  '[ "$(grep -c "card_names_no_other" scripts/image/check-h700.sh)" = 3 ] && grep -q "grub_names_card" scripts/image/check-h700.sh && grep -q "offer_names_card" scripts/image/check-h700.sh'

echo
if [ $fails -eq 0 ]; then echo "all card-texts tests passed"; else echo "$fails card-texts tests failed"; exit 1; fi
