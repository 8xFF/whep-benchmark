FROM ubuntu:22.04 as base
ARG TARGETPLATFORM
COPY . /tmp
WORKDIR /tmp

RUN echo $TARGETPLATFORM
RUN ls -R /tmp/
# move the binary to root based on platform
RUN case $TARGETPLATFORM in \
        "linux/amd64")  BUILD=x86_64-unknown-linux-gnu  ;; \
        "linux/arm64")  BUILD=aarch64-unknown-linux-gnu  ;; \
        *) exit 1 ;; \
    esac; \
    mv /tmp/$BUILD/whep-benchmark-$BUILD /whep-benchmark; \
    chmod +x /whep-benchmark;

FROM ubuntu:22.04

COPY --from=base /whep-benchmark /whep-benchmark

ENTRYPOINT ["/whep-benchmark"]
