# Развёртывание trackflow

Один контейнер с сервером и веб-интерфейсом внутри плюс PostgreSQL — [`Dockerfile`](Dockerfile) и [`compose.server.yaml`](compose.server.yaml). Рассчитано на машину профиля 2 vCPU / 4 ГБ.

## Собрать образ

На машине с Docker и исходниками (для сервера x86‑64; замените платформу на `linux/arm64`, если сервер на ARM):

```bash
docker buildx build --platform linux/amd64 -f deploy/Dockerfile -t trackflow:1.12c --load .
docker save trackflow:1.12c | gzip > trackflow-1.12c.tar.gz
```

## Развернуть на сервере

Нужны `trackflow-1.12c.tar.gz` и `deploy/compose.server.yaml`. Рядом с compose-файлом создайте `.env`:

```bash
POSTGRES_PASSWORD=$(openssl rand -hex 16)        # пароль базы
TRACKFLOW_SECRET_KEY=$(openssl rand -base64 32)  # ключ для хранения секретов почты
```

Затем:

```bash
docker load < trackflow-1.12c.tar.gz
docker compose -f compose.server.yaml up -d --wait
docker compose -f compose.server.yaml logs app   # строка «… enter the setup code …»
```

Откройте адрес в браузере и пройдите мастер настройки (код установки — из журнала). Подробности: [docs/guide/admin.md](../docs/guide/admin.md) («Первый запуск», «Почта и уведомления»).

По умолчанию порт `7700` открыт только на самом сервере (`127.0.0.1:7700`) — поставьте впереди reverse proxy с HTTPS (nginx/caddy) и передайте доверие к `X-Forwarded-For` через `TRACKFLOW_TRUSTED_PROXIES`. Для теста без прокси поменяйте публикацию порта на `"7700:7700"`.

## Обновление и резервные копии

- **Обновление**: собрать новый образ, на сервере `docker load` и снова `docker compose -f compose.server.yaml up -d` — миграции применятся при старте сами.
- **Резервные копии** (встроенных пока нет, ADM-001 в P2) — по крону, включают и базу, и том файлов:

```bash
docker compose -f compose.server.yaml exec db pg_dump -U trackflow -Fc trackflow > db-$(date +%F).dump
docker run --rm -v trackflow_appdata:/files -v "$PWD":/backup alpine \
  tar czf /backup/files-$(date +%F).tar.gz -C /files .
```

Перед обновлением — снапшот обеих копий; откат = старый образ плюс восстановленный дамп.
