// Grab one frame from the projector's built-in camera (UVC, 640x480 YUYV only)
// and write it as an upright greyscale PGM.
//   grab [device] [out.pgm] [exposure]      defaults: /dev/video0 /data/local/tmp/cam.pgm 130
// Auto exposure gives blank frames on this sensor, so exposure is set manually
// (130 ~ a white wall at normal brightness; raise it in a dark room).
// The sensor is mounted 90 degrees off, so the image is rotated counter-clockwise.
#include <errno.h>
#include <fcntl.h>
#include <linux/videodev2.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/mman.h>
#include <unistd.h>

#define W 640
#define H 480
#define NBUF 4

static int xioctl(int fd, unsigned long req, void *arg) {
  int r;
  do r = ioctl(fd, req, arg); while (r == -1 && errno == EINTR);
  return r;
}

static void set_ctrl(int fd, unsigned id, int value) {
  struct v4l2_control c = {.id = id, .value = value};
  if (xioctl(fd, VIDIOC_S_CTRL, &c) < 0) fprintf(stderr, "ctrl 0x%08x: %s\n", id, strerror(errno));
}

int main(int argc, char **argv) {
  const char *dev = argc > 1 ? argv[1] : "/dev/video0";
  const char *out = argc > 2 ? argv[2] : "/data/local/tmp/cam.pgm";
  int exposure = argc > 3 ? atoi(argv[3]) : 130;

  int fd = open(dev, O_RDWR);
  if (fd < 0) { perror(dev); return 1; }

  struct v4l2_format f = {.type = V4L2_BUF_TYPE_VIDEO_CAPTURE};
  f.fmt.pix.width = W; f.fmt.pix.height = H;
  f.fmt.pix.pixelformat = V4L2_PIX_FMT_YUYV; f.fmt.pix.field = V4L2_FIELD_ANY;
  if (xioctl(fd, VIDIOC_S_FMT, &f) < 0) { perror("S_FMT"); return 2; }
  if (f.fmt.pix.width != W || f.fmt.pix.height != H) {
    fprintf(stderr, "camera gave %ux%u, expected %dx%d\n", f.fmt.pix.width, f.fmt.pix.height, W, H);
    return 2;
  }

  set_ctrl(fd, V4L2_CID_EXPOSURE_AUTO, V4L2_EXPOSURE_MANUAL);
  set_ctrl(fd, V4L2_CID_EXPOSURE_ABSOLUTE, exposure);

  struct v4l2_requestbuffers rb = {.count = NBUF, .type = f.type, .memory = V4L2_MEMORY_MMAP};
  if (xioctl(fd, VIDIOC_REQBUFS, &rb) < 0) { perror("REQBUFS"); return 3; }
  unsigned char *buf[NBUF];
  for (unsigned i = 0; i < rb.count && i < NBUF; i++) {
    struct v4l2_buffer b = {.type = f.type, .memory = V4L2_MEMORY_MMAP, .index = i};
    if (xioctl(fd, VIDIOC_QUERYBUF, &b) < 0) { perror("QUERYBUF"); return 4; }
    buf[i] = mmap(NULL, b.length, PROT_READ | PROT_WRITE, MAP_SHARED, fd, b.m.offset);
    if (buf[i] == MAP_FAILED) { perror("mmap"); return 4; }
    if (xioctl(fd, VIDIOC_QBUF, &b) < 0) { perror("QBUF"); return 5; }
  }
  int type = f.type;
  if (xioctl(fd, VIDIOC_STREAMON, &type) < 0) { perror("STREAMON"); return 6; }

  // let exposure settle, keep the last frame
  struct v4l2_buffer b;
  for (int k = 0; k < 30; k++) {
    b = (struct v4l2_buffer){.type = f.type, .memory = V4L2_MEMORY_MMAP};
    if (xioctl(fd, VIDIOC_DQBUF, &b) < 0) { perror("DQBUF"); return 7; }
    if (k < 29 && xioctl(fd, VIDIOC_QBUF, &b) < 0) { perror("QBUF"); return 8; }
  }

  // YUYV: Y at every even byte. Rotate CCW: out(x', y') = in(W-1-y', x'), out is H wide, W tall.
  const unsigned char *p = buf[b.index];
  static unsigned char img[W * H];
  unsigned long sum = 0;
  for (int oy = 0; oy < W; oy++)
    for (int ox = 0; ox < H; ox++) {
      unsigned char y = p[(ox * W + (W - 1 - oy)) * 2];
      img[oy * H + ox] = y;
      sum += y;
    }

  FILE *o = fopen(out, "wb");
  if (!o) { perror(out); return 9; }
  fprintf(o, "P5\n%d %d\n255\n", H, W);
  fwrite(img, 1, sizeof img, o);
  fclose(o);
  printf("wrote %s (%dx%d, mean brightness %lu)\n", out, H, W, sum / (W * H));
  xioctl(fd, VIDIOC_STREAMOFF, &type);
  close(fd);
  return 0;
}
