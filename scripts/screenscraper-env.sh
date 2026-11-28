# Sourced by the scripts that build release programs (release-app.sh, android/build-apk.sh; the release kit's Air build):
# `. "$ROOT/scripts/screenscraper-env.sh"`. Exports pocket-ui's ScreenScraper developer id for build.rs
# (POCKETUI_SCREENSCRAPER_DEVID, POCKETUI_SCREENSCRAPER_DEVPASSWORD, POCKETUI_SCREENSCRAPER_SOFTNAME; src/screenscraper.rs, docs/SCREENSCRAPER.md)
# from the build machine's file, never from the repository:
#   ${SCREENSCRAPER_DEV_FILE:-$HOME/.config/screenscraper-dev}, mode 600, lines DEVID=..., DEVPASSWORD=..., SOFTNAME=... (optional).
# The file is read, not run, and nothing of it is printed: only whether the id is in the build. Without the file the build
# has no ScreenScraper (no row, no request), as every developer's and the gate's build.
_ss_file=${SCREENSCRAPER_DEV_FILE:-$HOME/.config/screenscraper-dev}
if [ -r "$_ss_file" ]; then
  while IFS= read -r _ss_line || [ -n "$_ss_line" ]; do
    _ss_line=$(printf '%s' "$_ss_line" | tr -d '\r')
    case $_ss_line in
      DEVID=*) POCKETUI_SCREENSCRAPER_DEVID=${_ss_line#DEVID=}; export POCKETUI_SCREENSCRAPER_DEVID ;;
      DEVPASSWORD=*) POCKETUI_SCREENSCRAPER_DEVPASSWORD=${_ss_line#DEVPASSWORD=}; export POCKETUI_SCREENSCRAPER_DEVPASSWORD ;;
      SOFTNAME=*) POCKETUI_SCREENSCRAPER_SOFTNAME=${_ss_line#SOFTNAME=}; export POCKETUI_SCREENSCRAPER_SOFTNAME ;;
    esac
  done < "$_ss_file"
  if [ -n "${POCKETUI_SCREENSCRAPER_DEVID:-}" ] && [ -n "${POCKETUI_SCREENSCRAPER_DEVPASSWORD:-}" ]; then
    echo "screenscraper: developer id in this build" >&2
  else
    echo "screenscraper: $_ss_file lacks DEVID or DEVPASSWORD, left out of this build" >&2
  fi
else
  echo "screenscraper: no developer id file, left out of this build" >&2
fi
unset _ss_file _ss_line
