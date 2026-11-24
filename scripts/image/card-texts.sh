# Which handheld a card's own texts name (pocket-ui-info/SOURCE-OFFER.txt, README.txt, boot/grub/grub.cfg). Some templates
# are written for one model of their family: deploy/sd-efi/grub.cfg and deploy/image/SOURCE-OFFER.txt.in for the Mini V2,
# deploy/sd-efi-sm8550/grub.cfg for the Pocket 6. The builders (scripts/make-image.sh, scripts/make-image-family.sh) turn them
# into the card's own with the filters below, and the image checks (scripts/image/check.sh, check-h700.sh) make sure the
# card names its model and image and no other model of the family. 0.6.0's Pocket 5 and Flip 2 cards said "Retroid Pocket
# Mini V2" in SOURCE-OFFER.txt and grub.cfg, because only the tree's file name was put in. Tested by
# scripts/image/test-card-texts.sh. Sourced, not run; needs pins.sh.

# sm8250_card_text, a filter: the Mini V2 template's model name, image name and tree for this card's (sm8250_pins:
# SM8250_MODEL, SM8250_MODEL_NAME, SM8250_DTB_FILE). On the Mini V2 card it changes nothing.
sm8250_card_text() {
  sed -e "s/miniv2-noufs\.dtb/$SM8250_DTB_FILE/g" -e "s/-rp-mini-v2\.img\.xz/-$SM8250_MODEL.img.xz/g" \
    -e "s/Retroid Pocket Mini V2/$SM8250_MODEL_NAME/g"
}

# sm8550_efi_card_text, a filter: the same for the SM8550 Retroids' GRUB files, written for the Pocket 6 (sm8550_efi_pins:
# EFI_MODEL_NAME, EFI_DTB_FILE).
sm8550_efi_card_text() {
  sed -e "s/rp6-noufs\.dtb/$EFI_DTB_FILE/g" -e "s/Retroid Pocket 6/$EFI_MODEL_NAME/g"
}

# card_models FAMILY: "key|name" for every model of the family: SM8250 (SM8250_MODELS) or a family_pins one (<FAMILY>_MODELS).
card_models() {
  if [ "$1" = SM8250 ]; then
    echo "$SM8250_MODELS" | awk -F '|' 'NF { print $1 "|" $2 }'
  else
    eval "ct_list=\${${1}_MODELS:-}"
    echo "$ct_list" | awk 'NF { n = $4; gsub(/_/, " ", n); print $1 "|" n }'
  fi
}

# card_other_words FAMILY MODEL: the words that name another model of the family, one per line: its name (unless it is this
# model's too: a DDR3 or new-screen card has its model's name), its image "-<key>.img.xz" and its tree "<key>-noufs.dtb"; for
# the SM8250 also the short names the notes use.
card_other_words() {
  ct_mine=$(card_models "$1" | awk -F '|' -v m="$2" '$1 == m { print $2 }')
  card_models "$1" | while IFS='|' read -r ct_k ct_n; do
    [ "$ct_k" != "$2" ] || continue
    [ "$ct_n" = "$ct_mine" ] || echo "$ct_n"
    echo "-$ct_k.img.xz"
    echo "$ct_k-noufs.dtb"
    [ "$1" = SM8250 ] || continue
    case $ct_k in
      rp-mini-v2) echo "Mini V2"; echo miniv2-noufs.dtb ;;
      rp5) echo "Pocket 5" ;;
      flip2) echo "Flip 2"; echo Flip2 ;;
    esac
  done
}

# card_names_no_other FILE FAMILY MODEL: FILE names no other model of the family (card_other_words). A word that holds this
# model's name ("Retroid Pocket 6 TOP-DPAD" on the Pocket 6's card) is looked for as it is; any other word only outside this
# model's name ("Retroid Pocket 6" on the TOP-DPAD's card, "Flip2" on the Flip 2's). Prints what it found.
card_names_no_other() {
  ct_own=$(card_models "$2" | awk -F '|' -v m="$3" '$1 == m { print $2 }')
  [ -n "$ct_own" ] && [ -f "$1" ] || return 1
  ct_rest=$(awk -v own="$ct_own" '{ while ((i = index($0, own)) > 0) $0 = substr($0, 1, i - 1) "|" substr($0, i + length(own)); print }' "$1")
  ct_bad=$(card_other_words "$2" "$3" | while IFS= read -r ct_w; do
    case $ct_w in
      *"$ct_own"*) grep -qF -- "$ct_w" "$1" && echo "\"$ct_w\"" ;;
      *) printf '%s\n' "$ct_rest" | grep -qF -- "$ct_w" && echo "\"$ct_w\"" ;;
    esac
  done)
  [ -z "$ct_bad" ] || { echo "     names another model: $(echo $ct_bad)"; return 1; }
}

# offer_names_card FILE VERSION MODEL_NAME IMAGE: SOURCE-OFFER.txt's title names this model and its written offer this
# card's image (IMAGE without .img.xz: pocket-ui-<version>-<key>).
offer_names_card() {
  [ "$(head -1 "$1")" = "Source code for the pocket-ui $2 SD card image ($3)" ] && grep -qxF "pocket-ui $2, $4.img.xz." "$1"
}

# grub_names_card FILE MODEL_NAME DTB: the first comment of grub.cfg names this model, and the line that lists the tree
# names this model's tree and model.
grub_names_card() {
  head -1 "$1" | grep -qF "# pocket-ui on the $2" &&
    grep -E '^#' "$1" | grep -F "/boot/pocket/$3 " | grep -qF "$2"
}
