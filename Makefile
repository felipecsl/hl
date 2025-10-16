# https://github.com/llimllib/node-esbuild-executable/blob/main/Makefile
# Makefile for building a single-file executable Node.js app using esbuild and postject

NODE_BIN ?= $(shell asdf which node 2>/dev/null || nvm which node 2>/dev/null || command -v node)
UNAME_S := $(shell uname -s)

# build the `hl` binary
#
# https://nodejs.org/api/single-executable-applications.html
#
# $@ means "the name of this target", which is "dist/hl" in this case
dist/hl: dist/hl.js
	node --experimental-sea-config sea-config.json
	cp $(NODE_BIN) $@
	strip $@
ifeq ($(UNAME_S),Darwin)
	codesign --remove-signature $@
	npx postject $@ NODE_SEA_BLOB sea-prep.blob \
		--sentinel-fuse NODE_SEA_FUSE_fce680ab2cc467b6e072b8b5df1996b2 \
		--macho-segment-name NODE_SEA
	codesign --sign - $@
else
	npx postject $@ NODE_SEA_BLOB sea-prep.blob \
		--sentinel-fuse NODE_SEA_FUSE_fce680ab2cc467b6e072b8b5df1996b2
endif

.PHONY: clean
clean:
	rm dist/*