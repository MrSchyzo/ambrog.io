FROM ubuntu:22.04
RUN apt update && apt install -y openssl ca-certificates ffmpeg yt-dlp && update-ca-certificates

RUN ffmpeg --help
RUN yt-dlp --help

RUN mkdir -p /workdir
WORKDIR /workdir

COPY target/release/ambrogio_bin /workdir/ambrogio
RUN chmod +x /workdir/ambrogio && mkdir -p /workdir/storage
ARG app_version=unknown
ENV APP_VERSION=${app_version}

ENTRYPOINT /workdir/ambrogio
