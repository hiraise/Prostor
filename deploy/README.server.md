# Развёртывание prostor

Один контейнер с сервером и веб-интерфейсом внутри плюс PostgreSQL — [`Dockerfile`](Dockerfile) и [`compose.server.yaml`](compose.server.yaml). Рассчитано на машину профиля 2 vCPU / 4 ГБ.

Готовый образ публикуется в реестре: `ghcr.io/hiraise/prostor` (теги вида `0.1.0` и `latest` — релизы, `main` — текущее состояние разработки). Обе архитектуры: `linux/amd64` и `linux/arm64` — Docker сам выберет нужную.

## Установить

Скачайте [`compose.server.yaml`](compose.server.yaml) и рядом создайте `.env`:

```bash
POSTGRES_PASSWORD=$(openssl rand -hex 16)        # пароль базы
PROSTOR_SECRET_KEY=$(openssl rand -base64 32)  # ключ для хранения секретов почты
```

Затем:

```bash
docker compose -f compose.server.yaml pull
docker compose -f compose.server.yaml up -d --wait
docker compose -f compose.server.yaml logs app   # строка «… enter the setup code …»
```

Откройте адрес в браузере и пройдите мастер настройки (код установки — из журнала). Подробности: [docs/guide/admin.md](../docs/guide/admin.md) («Первый запуск», «Почта и уведомления»).

По умолчанию порт `7700` открыт только на самом сервере (`127.0.0.1:7700`) — поставьте впереди reverse proxy с HTTPS (nginx/caddy) и передайте доверие к `X-Forwarded-For` через `PROSTOR_TRUSTED_PROXIES`. Для теста без прокси поменяйте публикацию порта на `"7700:7700"`.

## Обновление и резервные копии

- **Обновление**: `docker compose -f compose.server.yaml pull && docker compose -f compose.server.yaml up -d` — миграции применятся при старте сами. Обновляться можно и на `main`, но релизные теги стабильнее.
- **Резервные копии** (встроенных пока нет, ADM-001 в P2) — по крону, включают и базу, и том файлов:

```bash
docker compose -f compose.server.yaml exec db pg_dump -U prostor -Fc prostor > db-$(date +%F).dump
docker run --rm -v prostor_appdata:/files -v "$PWD":/backup alpine \
  tar czf /backup/files-$(date +%F).tar.gz -C /files .
```

Перед обновлением — снапшот обеих копий; откат = старый образ плюс восстановленный дамп.

## Собрать образ из исходников

Для машины без доступа к реестру или своих правок (для сервера x86‑64; замените платформу на `linux/arm64`, если сервер на ARM):

```bash
docker buildx build --platform linux/amd64 -f deploy/Dockerfile -t prostor:local --load .
```

и замените `image:` в `compose.server.yaml` на `prostor:local`. Обновление — пересборка и `docker compose -f compose.server.yaml up -d`.
