/* dropbear for pocket-ui's TrimUI Brick card (docs/TRIMUI-BRICK-REMOTE.md). Only what Remote access needs: root logs in with the
 * device's password (DROPBEAR_PASSWORD_FILE, pocket-ui.patch) or a key from the card. */
#define DROPBEAR_X11FWD 0
#define DROPBEAR_SVR_AGENTFWD 0
#define DO_MOTD 0
#define DROPBEAR_SVR_PASSWORD_AUTH 1
#define DROPBEAR_SVR_PUBKEY_AUTH 1
/* No sftp-server on the card: `scp -O` (the old protocol) and `ssh brick cat FILE` work. */
#define DROPBEAR_SFTPSERVER 0
/* The card's tools first (tailscale, curl, scp; start.sh links them into /tmp/pocket-ui/bin), then the firmware's. */
#define DEFAULT_ROOT_PATH "/tmp/pocket-ui/bin:/usr/sbin:/usr/bin:/sbin:/bin"
