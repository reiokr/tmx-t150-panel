obj-m += hid-t150.o
KDIR ?= /lib/modules/$(shell uname -r)/build
LLVM_FLAG := $(shell grep -qs '^CONFIG_CC_IS_CLANG=y' $(KDIR)/.config && echo LLVM=1)

all:
	$(MAKE) -C $(KDIR) M=$(shell pwd) $(LLVM_FLAG) modules

clean:
	$(MAKE) -C $(KDIR) M=$(shell pwd) clean
