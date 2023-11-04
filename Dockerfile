FROM ubuntu:22.04
RUN apt update --fix-missing && \
    apt install -y software-properties-common && \
    add-apt-repository ppa:tomtomtom/yt-dlp && \
    apt update --fix-missing && \
    apt install -y openssl ca-certificates ffmpeg yt-dlp && \
    update-ca-certificates

RUN ffmpeg --help
RUN yt-dlp --help

RUN mkdir -p /workdir
WORKDIR /workdir

COPY target/release/ambrogio_bin /workdir/ambrogio
RUN chmod +x /workdir/ambrogio && mkdir -p /workdir/storage
ARG app_version=unknown
ENV APP_VERSION=${app_version}

# Enforce upgrades
ENTRYPOINT ["/bin/sh", "-c" , "apt install -y --only-upgrade ffmpeg yt-dlp && /workdir/ambrogio"]
