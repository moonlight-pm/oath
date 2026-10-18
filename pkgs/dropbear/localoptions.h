/* Oath: OpenSSH's sftp-server and a /bin-only PATH for sessions. */
#define SFTPSERVER_PATH "/bin/sftp-server"
#define DEFAULT_PATH "/bin"
#define DEFAULT_ROOT_PATH "/bin"
/* Logins are by key (ssh:local); glibc has no crypt() since 2.39. */
#define DROPBEAR_SVR_PASSWORD_AUTH 0
