FROM electronuserland/builder:wine

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        curl \
        gh \
        git \
        imagemagick \
        jq \
        libasound2 \
        libatk-bridge2.0-0 \
        libatk1.0-0 \
        libgbm1 \
        libgtk-3-0 \
        libnss3 \
        libxss1 \
        librsvg2-bin \
        xvfb \
    && rm -rf /var/lib/apt/lists/*
