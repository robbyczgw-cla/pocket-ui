#include <linux/input.h>
#include <sys/ioctl.h>
#include <fcntl.h>
#include <unistd.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
static void quoted(const char *s) { putchar('"'); for(;*s;s++){ if(*s=='"'||*s=='\\')putchar('\\'); if((unsigned char)*s>=32)putchar(*s); }putchar('"'); }
int main(int argc,char **argv) {
 puts("[");
 for(int n=1;n<argc;n++) {
  int f=open(argv[n],O_RDONLY|O_NONBLOCK); if(f<0)return 2;
  char name[256]={0}; struct input_id id; unsigned char keys[128]={0},abs[8]={0},ev[8]={0};
  if(ioctl(f,EVIOCGNAME(sizeof name),name)<0||ioctl(f,EVIOCGID,&id)<0)return 3;
  ioctl(f,EVIOCGBIT(EV_KEY,sizeof keys),keys);ioctl(f,EVIOCGBIT(EV_ABS,sizeof abs),abs);ioctl(f,EVIOCGBIT(0,sizeof ev),ev);
  if(n>1)puts(",");
  printf("{\"name\":");quoted(name);
  printf(",\"id\":{\"bustype\":%u,\"vendor\":%u,\"product\":%u,\"version\":%u},\"keys\":[",id.bustype,id.vendor,id.product,id.version);
  int first=1;for(int k=0;k<768;k++)if(keys[k/8]&(1<<(k%8))){if(!first)putchar(',');printf("%d",k);first=0;}
  printf("],\"abs\":[");first=1;for(int k=0;k<64;k++)if(abs[k/8]&(1<<(k%8))){struct input_absinfo a;if(ioctl(f,EVIOCGABS(k),&a)<0)return 4;if(!first)putchar(',');printf("[%d,%d,%d,%d,%d,%d]",k,a.minimum,a.maximum,a.fuzz,a.flat,a.resolution);first=0;}
  printf("],\"bitmaps\":{\"ev_bytes_le\":\"");for(int i=0;i<8;i++)printf("%02x",ev[i]);printf("\",\"key_bytes_le\":\"");for(int i=0;i<96;i++)printf("%02x",keys[i]);printf("\",\"abs_bytes_le\":\"");for(int i=0;i<8;i++)printf("%02x",abs[i]);printf("\"}}");close(f);
 }puts("\n]");return 0;
}
