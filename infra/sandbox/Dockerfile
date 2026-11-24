# Immagine del terminale sandbox di JARVIS (ADR-0105). Costruzione: infra/sandbox/build.sh
FROM python:3.13-alpine
RUN apk add --no-cache bash coreutils findutils grep sed gawk jq ripgrep git curl nodejs npm \
        build-base sqlite tar gzip unzip zip diffutils less
ENV PYTHONDONTWRITEBYTECODE=1 PYTHONUNBUFFERED=1 PIP_NO_CACHE_DIR=1 PIP_DISABLE_PIP_VERSION_CHECK=1 \
    PIP_BREAK_SYSTEM_PACKAGES=1
WORKDIR /work
