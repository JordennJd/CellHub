# Сетевой режим: server-authoritative multiplayer

Сетевой режим **опционален**. Без него приложение работает как раньше: React-редактор,
Rust/WASM-симуляция в Worker и локальный рендерер. Для совместной сцены запускается
отдельный Rust-сервер, который владеет единственной копией мира и общается с браузерами
по WebSocket.

```
браузер A ── WebSocket ─┐
                        ├─ cellhub-server ─ command queue ─ Simulation (Rust/hecs)
браузер B ── WebSocket ─┘        │
                                 └─ один чанк карты за тик → broadcast всем
```

## Что принадлежит кому

| Данные | Владелец | Передаётся |
| --- | --- | --- |
| Геном в редакторе, архетипы, выбранный режим | каждый клиент | нет |
| `spawn`/`edit` с геномом | клиент отправляет команду | JSON-команда |
| Клетки, связи, пища, свет, радиация, tick | сервер | бинарные чанки |
| Позиции между чанками | клиент предсказывает | локально, до следующего чанка |

У каждого игрока свой редактор генома. Общая только сцена: сервер принимает геном в
команде `spawn` и хранит его у конкретной клетки. `inspect` возвращает геном выбранной
клетки, поэтому клиент может загрузить чужую клетку в свой локальный редактор, но это
не меняет редактор других игроков. Команда `edit` меняет одну клетку сцены.

## Публичные комнаты

1. Клиент открывает WebSocket на `/ws`.
2. `{"type":"list"}` возвращает публичные комнаты с населением, числом игроков и тиком.
3. `{"type":"create","name":"...","player":"..."}` создаёт комнату и **начинает
   новый сценарий**: сервер поднимает отдельный пустой `Simulation` со свежим seed,
   запускает его с фиксированным шагом 30 Гц и делает создателя хостом. Локальная
   сцена браузера и редактор генома в комнату не переносятся.
4. `{"type":"join","room":"...","player":"..."}` подключает к уже идущей симуляции.
   Новые клиенты получают сцену постепенно, чанк за чанком.
5. Пустая комната живёт ещё 30 секунд и затем закрывается.

Очистить мир (`reset`) может только создатель комнаты. Спавн и редактирование клеток
доступны всем участникам; лимит 20 000 клеток и валидация генома остаются серверными.

## Последовательная синхронизация карты

Мир 3072 × 3072 разбит на **6 × 6 = 36 чанков** по 512 × 512 единиц. Сервер на каждом
тике формирует ровно один чанк и рассылает его всем клиентам:

* клетки, попавшие в чанк, — stride 12 float32;
* связи, у которых меньший ID находится в этом чанке, — пары `u32`;
* 8 × 8 участков субстрата этого чанка — 4 float32 на участок.

Чанк содержит текущий `tick`, население, рождения, смерти и заражения. Полный круг —
36 тиков, около 1.2 секунды при 30 Гц. Клиент не получает «весь мир сразу»: до прихода
очередного чанка он показывает последнее известное состояние и постепенно заполняет
карту. По той же причине смерть клетки может оставаться видимой до одного полного
цикла, пока её чанк не обновится.

### Бинарный формат чанка

Little-endian, заголовок 31 байт:

| Смещение | Размер | Поле |
| --- | --- | --- |
| 0 | 1 | тип `1` (chunk) |
| 1 | 4 | `tick` |
| 5 | 4 | `population` |
| 9 | 4 | `births` |
| 13 | 4 | `deaths` |
| 17 | 4 | `infections` |
| 21 | 1 | `chunkX` |
| 22 | 1 | `chunkY` |
| 23 | 2 | число клеток |
| 25 | 2 | число связей |
| 27 | 2 | число участков субстрата |
| 29 | 2 | резерв |
| 31 | `cellCount × 48` | клетки, 12 float32 |
| далее | `linkCount × 8` | связи, 2 u32 |
| далее | `tileCount × 16` | субстрат, 4 float32 |

Строка клетки: `[id, x, y, radius, kind, mass, age, generation, mutations, angle,
effectiveSpeed, linkedFlag]`. Стандартный рендерер использует первые 10 значений;
`effectiveSpeed` и `linkedFlag` нужны только клиентскому предсказанию.

Управляющие сообщения — JSON с полем `type`. Клиентские команды содержат монотонный
`seq`; сервер отвечает `ack {seq, cellId?, error?}`, `inspect` или `error`.

## Предсказание и «прогибание» под сервер

Клиент не запускает вторую биологическую симуляцию и не пытается переиграть сервер.
Вместо этого `frontend/net/prediction.ts` поддерживает предсказанную сцену:

* пришедший чанк **безусловно перезаписывает** позиции, массу, тип и связи своих клеток;
* между чанками свободные клетки двигаются по последней известной скорости и углу;
* для связанных клеток 12 проходов решают те же позиционные ограничения, что и в Rust;
* предсказание затухает через 1.6 секунды без обновления, чтобы не улететь;
* локальный `spawn` сразу показывается клеткой-призраком с отрицательным ID;
* `ack` связывает призрака с настоящим ID, и призрак исчезает, как только серверная
  клетка появляется в чанке; ошибка команды удаляет призрака;
* `world_reset` очищает локальную сцену.

Если сервер прислал другой результат, клиент не спорит: серверное значение имеет
приоритет. Биология, деление, заражение, мутации и смерть существуют только на сервере.

## Запуск

```sh
# терминал 1: авторитарный сервер + WebSocket на :8082
just server
# или
cargo run -p cellhub-server -- --bind 127.0.0.1:8082 --static dist

# терминал 2: Vite; /ws проксируется на :8082
npm run dev
```

Откройте страницу, нажмите **«Сеть»**, подключитесь к `ws://127.0.0.1:8082/ws`
(в dev-режиме адрес по умолчанию уже указывает на Vite `/ws`), создайте комнату или
войдите в существующую. Для production сервер сам раздаёт `dist/`:

```sh
npm run build
cargo run --release -p cellhub-server -- --bind 0.0.0.0:8082 --static dist
```

Штатный деплой на VM:

```sh
# один раз: systemd-юнит и узкое sudo-правило (нужен root SSH)
just bootstrap cellhub.obscurative.ru

# дальше — обычный деплой от пользователя cellhub
just deploy cellhub.obscurative.ru
# если SSH-ключ не выбирается автоматически:
CELLHUB_SSH_KEY=~/.ssh/cellhub-github-actions_ed25519 just deploy cellhub.obscurative.ru
```

Рецепт собирает `dist/` и `target/deploy/cellhub-server` — статический
`aarch64-unknown-linux-musl` бинарник. Целевой `rust-std` объявлен в `flake.nix`
и `rust-toolchain.toml`, поэтому dev shell сразу содержит нужную стандартную
библиотеку. На VM он копирует статику в `/var/lib/cellhub`, бинарник — в
`/var/lib/cellhub/.local/bin/cellhub-server` и перезапускает system-сервис
`cellhub.service` на `0.0.0.0:8081`. Nix-ld и glibc на VM не нужны; включённый
`programs.nix-ld` не мешает, потому что бинарник статический.

`cellhub.service` — обычный system-юнит, а не user-сервис, поэтому linger не
нужен. `just bootstrap` один раз ставит юнит и правило sudo, которое разрешает
пользователю `cellhub` только `systemctl stop|start|restart cellhub.service`.
На обычной NixOS-машине скрипт пишет `/etc/systemd/system/cellhub.service`; на
shell-VM этот каталог — read-only symlink в Nix store, поэтому скрипт ставит
runtime drop-in в `/run/systemd/system/cellhub.service.d/override.conf`, а
правило sudo добавляет в `/etc/sudoers`. Runtime drop-in исчезает при
перезагрузке VM, поэтому сервис нужно объявить в NixOS-конфигурации постоянно.
В текущем кластере shell-VM описана в `modules/shell-vm.nix` репозитория
`opi5-server`; изменение применяется штатным `ACTION=test ./deploy.sh`
(или `ACTION=switch` после подтверждения). Пример постоянной конфигурации:

```nix
systemd.services.cellhub = {
  description = "CellHub authoritative server (:8081)";
  wantedBy = [ "multi-user.target" ];
  after = [ "network.target" ];
  serviceConfig = {
    User = "cellhub";
    Group = "cellhub";
    WorkingDirectory = "/var/lib/cellhub";
    ExecStart = "/var/lib/cellhub/.local/bin/cellhub-server --bind 0.0.0.0:8081 --static /var/lib/cellhub --max-rooms 16 --max-players 16";
    Environment = [
      "RUST_LOG=cellhub_server=info,tower_http=warn"
      "HOME=/var/lib/cellhub"
    ];
    Restart = "on-failure";
    RestartSec = 2;
    LimitNOFILE = 65536;
    NoNewPrivileges = true;
    PrivateTmp = true;
  };
};

security.sudo.extraRules = [{
  users = [ "cellhub" ];
  commands = [
    { command = "/run/current-system/sw/bin/systemctl stop cellhub.service"; options = [ "NOPASSWD" ]; }
    { command = "/run/current-system/sw/bin/systemctl start cellhub.service"; options = [ "NOPASSWD" ]; }
    { command = "/run/current-system/sw/bin/systemctl restart cellhub.service"; options = [ "NOPASSWD" ]; }
  ];
}];

networking.firewall.allowedTCPPorts = [ 8081 ];
```

Основной Caddy терминирует TLS и проксирует весь домен на VM:

```caddyfile
cellhub.obscurative.ru {
    encode zstd gzip
    reverse_proxy 10.200.0.7:8081
}
```

Caddy 2 сам пробрасывает WebSocket Upgrade, дополнительные заголовки не нужны.
Не добавляйте `transport http { versions h2c }` и не удаляйте `Upgrade`/`Connection`
другими middleware. Порт 8081 должен быть открыт в файрволе VM только для
основной машины.

GitHub Actions использует secrets `DEPLOY_SSH_KEY`, `DEPLOY_KNOWN_HOSTS` и
variables `DEPLOY_HOST`, `DEPLOY_PORT`, `DEPLOY_USER`. `DEPLOY_USER` должен быть
`cellhub`; CI перезапускает тот же system-сервис через узкое sudo-правило,
поэтому `just bootstrap` должен быть выполнен на VM заранее.

Параметры сервера: `--bind <addr>`, `--static <dir>`, `--max-rooms <n>` (по
умолчанию 64), `--max-players <n>` (по умолчанию 32). Рецепт `just deploy`
выставляет более скромные 16/16. Если перед сервером стоит не Caddy, а другой
reverse-proxy, он должен пробрасывать Upgrade/Connection для `/ws`.

## Ограничения

* Сервер и клиент используют float32, но кроссплатформенный lockstep не заявлен:
  авторитетен только серверный снимок.
* Один тик — один чанк: далёкие чанки обновляются реже, чем активные, и клиентская
  интерполяция между ними приблизительна.
* Нет аккаунтов, приватных комнат, постоянного мира и античита за пределами валидации
  команд и лимитов.
* Сетевой режим не использует SharedArrayBuffer и не требует COOP/COEP.
