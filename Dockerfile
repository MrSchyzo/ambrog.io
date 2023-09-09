FROM ubuntu:22.04
RUN apt install -y openssl

RUN mkdir -p /workdir
WORKDIR /workdir

COPY target/release/ambrogio_bin /workdir/ambrogio
RUN chmod +x /workdir/ambrogio

ENTRYPOINT /workdir/ambrogio
