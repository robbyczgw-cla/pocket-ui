/* Disposable emulator only: a real uinput device exercises Android's hot-plug listener.
 * Build with the NDK, then run as root with commands on stdin. Not part of the APK. */
#include <linux/uinput.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <sys/ioctl.h>
static void event(int fd,int type,int code,int value) {
    struct input_event e={0};e.type=type;e.code=code;e.value=value;
    if(write(fd,&e,sizeof e)!=sizeof e){perror("input event");exit(1);}
}
int main(void) {
    int fd=open("/dev/uinput",O_WRONLY|O_NONBLOCK);
    if(fd<0){perror("uinput");return 2;}
    ioctl(fd,UI_SET_EVBIT,EV_KEY);
    int keys[]={BTN_A,BTN_B,BTN_X,BTN_Y,BTN_TL,BTN_TR,BTN_START,BTN_SELECT};
    for(unsigned i=0;i<sizeof keys/sizeof *keys;i++)ioctl(fd,UI_SET_KEYBIT,keys[i]);
    ioctl(fd,UI_SET_EVBIT,EV_ABS);
    struct uinput_user_dev pad={0};strcpy(pad.name,"pocket-android-02 QA gamepad");pad.id.bustype=BUS_USB;pad.id.vendor=0x1234;pad.id.product=0x5678;
    int axes[]={ABS_X,ABS_Y,ABS_HAT0X,ABS_HAT0Y};
    for(unsigned i=0;i<sizeof axes/sizeof *axes;i++) {int a=axes[i];ioctl(fd,UI_SET_ABSBIT,a);pad.absmin[a]=a>=ABS_HAT0X?-1:-32768;pad.absmax[a]=a>=ABS_HAT0X?1:32767;}
    if(write(fd,&pad,sizeof pad)!=sizeof pad||ioctl(fd,UI_DEV_CREATE)<0){perror("create gamepad");return 1;}
    puts("gamepad ready");fflush(stdout);
    char line[80];
    while(fgets(line,sizeof line,stdin)) {
        int key,down;
        if(sscanf(line,"key %d %d",&key,&down)==2) {event(fd,EV_KEY,key,down);event(fd,EV_SYN,SYN_REPORT,0);}
        if(!strncmp(line,"quit",4))break;
    }
    ioctl(fd,UI_DEV_DESTROY);close(fd);return 0;
}
