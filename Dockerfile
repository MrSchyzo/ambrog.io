FROM ubuntu:26.04
RUN apt update --fix-missing && \
    apt install -y openssl ca-certificates ffmpeg wget python3 && \
    update-ca-certificates && \
    wget https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp -O /usr/local/bin/yt-dlp && \
    chmod a+rx /usr/local/bin/yt-dlp

RUN ffmpeg --help
RUN yt-dlp --help

RUN mkdir -p /workdir
WORKDIR /workdir

COPY target/release/ambrogio_bin /workdir/ambrogio
RUN chmod +x /workdir/ambrogio && mkdir -p /workdir/storage
ARG app_version=unknown
ENV APP_VERSION=${app_version}

# Enforce upgrades
ENTRYPOINT ["/bin/sh", "-c" , "apt update --fix-missing && apt install -y --only-upgrade ffmpeg && yt-dlp -U && /workdir/ambrogio"]
