# DKMS wrapper.  CachyOS/Arch kernels are built with clang, and such kernels
# reject the gcc-only flags recorded in their build tree: detect that and pass
# LLVM=1 automatically.
KDIR ?= /lib/modules/$(shell uname -r)/build
LLVM_FLAG := $(shell grep -qs '^CONFIG_CC_IS_CLANG=y' $(KDIR)/.config && echo LLVM=1)

all:
	mkdir -p build
	$(MAKE) -C ./hid-t150 all KDIR=$(KDIR) $(LLVM_FLAG)
	cp ./hid-t150/hid-t150.ko ./build/

clean:
	$(MAKE) -C ./hid-t150 clean KDIR=$(KDIR)
	rm -rf ./build/*
