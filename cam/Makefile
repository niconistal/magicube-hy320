# Static 32-bit ARM binary (the projector's userspace is armeabi-v7a only).
# Needs zig (https://ziglang.org) — or set CC to any arm-linux-musleabihf gcc.
CC ?= zig cc -target arm-linux-musleabihf
ifeq ($(origin CC),default)
CC = zig cc -target arm-linux-musleabihf
endif

grab: grab.c
	$(CC) -O2 -static -Wall -o $@ $<

clean:
	rm -f grab
