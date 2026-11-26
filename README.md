# ЮПИ Лаунчер (victory_upifork)

Сборка VIctory для **NVIDIA RTX 3060 12 ГБ и ПК с 16 ГБ оперативной памяти**
(Windows 11). Файлов игры здесь нет: нужна своя копия (Series 1.0.2.1).

## Установка и запуск

1. `git pull`, затем `.\Setup.cmd`. В первый раз он спросит, где лежит
   `game_scarlett_master.exe`, соберёт `victory.exe` (release) и поставит
   всё в `%LOCALAPPDATA%\UPI Launcher` (сохранения остаются в `state`).
2. Ярлыки с иконкой «VI» на рабочем столе и в меню «Пуск»:
   - **ЮПИ Лаунчер** — профиль Series S (меньше памяти, настройки игры ниже),
     настройки cool_fox и GTX 1660, без правок рендера: самый проверенный;
   - **ЮПИ Лаунчер (турбо)** — то же плюс правки рендера 1, 2 и 6 из
     avatarmanz/VIctory-7900XTX и 2 ГБ копий буферов в видеопамяти вместо ОЗУ:
     1 — графическая куча игры читается картой на месте (с ней рисуются
     лица), 2 — места записи дескрипторов сами отмечают слоты, 6 — общая
     блокировка ловушек записи. Правка 7 не включена: на 6 ядрах поток
     отправки ждал её ~40 мс на кадр; правка 5 — тоже: она переводит все
     шейдеры заново и проверена только на AMD. Плюс `VICTORY_INPLACE=1`:
     графическая память игры (пулы вершин, индексов, констант, ~3,9 ГБ)
     читается картой на месте, без 3,1–3,6 ГБ копий в ОЗУ; и «умное копирование
     назад» из Neptune (`VICTORY_SMART_COPY_BACK=1`, порог 4 КБ): то, что
     шейдеры пишут в буферы игры, остаётся в видеопамяти, пока игра сама не
     прочитает это копированием;
   - **ЮПИ Лаунчер (Series X)** — профиль Series X без правок (13,5 ГБ памяти
     игры: на 16 ГБ ОЗУ медленно, идёт подкачка);
   - **ЮПИ Лаунчер (безопасный)** — Series S без новейшей многопоточности:
     первое, что пробовать, если что-то не так.
3. Логи: `%LOCALAPPDATA%\UPI Launcher\logs` (в имени файла — режим), путь к
   последнему — в `latest.txt`. Первый запуск долгий: переводятся все шейдеры
   (кэш — `cache-coolfox`). Каждые 10 с в логе — строки памяти: сколько
   процесса в ОЗУ, сколько свободно, ошибки страниц в секунду, видеопамять
   и наша память по видам. `VICTORY_BLOBS_LOG=1` добавляет в лог (base64)
   шейдеры, которые не переводятся, и то, что игра передаёт методам
   `0x1c8`/`0x218`.

Пока игра запущена, лаунчер готовит ПК (как `neptune-turbo.ps1` из Neptune):
схема питания «высокая производительность», запись Game DVR выключена,
фоновые программы получают пониженный приоритет процессора и памяти и отдают
ОЗУ. После выхода из игры всё возвращается; `-NoPrep` отключает подготовку.

Управление без геймпада: клавиатура как геймпад (W A S D, I J K L, Enter или
пробел — A, Backspace — B, X, Y, стрелки, Q/E, Z/C, Shift/Ctrl — нажатия
стиков, Esc, Tab) и мышь (из Neptune): щелчок по картинке или F9 захватывает
мышь — движение крутит камеру, левая кнопка — RT, правая — LT; F9 или
переключение окна отпускает её. `VICTORY_MOUSE` — чувствительность (1).

Настройки лаунчера — в `launcher/UPI-Launcher.ps1` (переменная `VICTORY_*`,
заданная вручную, важнее): профиль cool_fox «pareto-residency», настройки
GTX 1660 (`CARD_BUFFERS_MB=1024`, `CARD_WINDOWS_MB=1536` — его совет для карт
от 8 ГБ: окна памяти игры, которые читают шейдеры, лежат в видеопамяти; ОЗУ
это не экономит, копия для процессора остаётся; `TEXTURE_MIP_DROP=2` — на 3 мягче, а
времени не экономит: текстуры стоят ~1 мс на кадр), `WINDOW_BUDGET_MB=1024`
для 16 ГБ.

Известно:
- без правки 1 на NVIDIA головы персонажей не рисуются или чёрные (то же
  видели в victory-gtx1660 на нетронутом cool_fox); с правкой 1 («турбо»)
  лица на месте;
- на 16 ГБ ОЗУ главный тормоз — подкачка: в мире процесс держит 15–19 ГБ,
  в ОЗУ из них 8,5–11 ГБ, свободной памяти у ПК остаётся 0,1–0,7 ГБ;
  файл подкачки на SSD спасает от вылетов, но не от замедления. Нужен файл
  подкачки 48 ГБ (49152 МБ): с маленьким у Windows остаётся меньше 1 ГБ на
  выделение памяти, и слой придерживает загрузку текстур (лаунчер
  предупреждает, если ОЗУ и подкачки вместе меньше 40 ГБ);
- не переводятся под 32-потоковые волны NVIDIA 3 шейдера:
  `CS_DefragHeap_ComputeRelocations` и два `CS_TransmissionVolumetrics_Tiled`
  (объёмный свет и дымка);
- метод `0x1c8` (~16 000 конвейеров) создаёт конвейер из готового кода GFX10
  с сигнатурой корня и метаданными, без обычного контейнера шейдера: не
  сделан ни в одном форке.

## Из чего собрано

- **[Dolberthi/neptune-dolberth-nvidia-8gb](https://github.com/Dolberthi/neptune-dolberth-nvidia-8gb)**
  (Neptune HyperEngine by Dolberth, другая ветка форков VIctory): логи пишутся
  отдельным потоком (при подкачке запись лога держала поток отправки до 1,76 с),
  мышь для камеры и курков, умное копирование назад, подготовка ПК.

- **[Pavl3000/cgbj-fork](https://github.com/Pavl3000/cgbj-fork)**, ветка
  `wave2/upstream`: уцелевшая копия codec-d/cool_fox (проверен на RTX 12 ГБ,
  32 ГБ ОЗУ) с первыми двумя волнами переносов.
- **[Impre-visible/victory-gtx1660](https://github.com/Impre-visible/victory-gtx1660)**:
  все шесть патчей (0001–0007) и настройки, давшие ~100 мс на кадр на GTX 1660.
- **[avatarmanz/VIctory-7900XTX](https://github.com/avatarmanz/VIctory-7900XTX)**
  `main` (`c9c932c`) целиком, слиянием: правки рендера 1, 2, 5, 6, 7, размер
  вывода и масштаб рендера (`VICTORY_OUTPUT`, `VICTORY_RENDER_SCALE`), перевод всех
  шейдеров игры, исправления для 32-потоковых волн (#142, #82, #66, #4),
  бюджет ОЗУ для «окон» (#77), переключатель частоты кадров, подсветка.
  Конфликты решены в пользу поведения cgbj (cool_fox) там, где оно проверено
  на NVIDIA: бюджеты памяти включены, `draw.rs` от cgbj.
- **[Belobug/VIctory-Belobug-AMD](https://github.com/Belobug/VIctory-Belobug-AMD)**:
  его история переписана; нужное (подсветка, пропуск лучей экранных зондов)
  уже есть в этом дереве. Проходы очистки шейдеров (2155 строк) отложены.
- **[fablinoo/kelvaroq](https://github.com/fablinoo/kelvaroq)**: отдельная
  архитектура; его новый слой памяти ещё не опубликован — брать пока нечего.

Авторы сохранены в истории коммитов. Прежняя основа (DexNom) — коммит `9cb06dc`.

---

> ## This fork
>
> A fork of **codec-d/cool_fox** at `f4b48ad` (26 November 2026), whose repository
> was deleted from GitHub — this tree is the surviving copy of its source. It runs
> an Xbox Series title natively on Windows from files the user supplies and
> contains no game files, code or assets.
>
> Work from **avatarmanz/VIctory-7900XTX** and from **Pavl3000/ztrx** is taken in
> one commit at a time, each naming its source and author. `docs/integration.md`
> says what has been taken, what was deliberately left out, and why. `main` is
> codec-d's tree as it was published; the work is on the `wave*` branches.
>
> Licence: GPL-3.0-or-later, as the trees it comes from. The credits below and in
> `LICENSE` stand; `crates/victory-formats/src/bcpack-tables.bin` is the one file
> the licence does not cover (`docs/asset-policy.md`).

# VIctory on an RX 7900 XTX

## Run the codec_d Windows build

Download and extract the Windows x64 ZIP from the [private releases](https://github.com/codec-d/cool_fox/releases/latest). Open PowerShell in the extracted folder and point the launcher at your own complete Xbox dump.

```powershell
$env:VICTORY_GAME_DIR = 'D:\Games\MyDump'
.\Play-VIctory.cmd pareto-residency
```

The launcher selects Series S and the RTX-tested residency settings. It builds its shader cache locally under `%LOCALAPPDATA%\VIctory\cache-pareto`. Set `VICTORY_CACHE_DIR` to reuse an existing cache outside the checkout. Windows x64, an AVX2 CPU, and a Direct3D 12 GPU with shader model 6.6 and mesh shader support are required.

To build from this checkout, install the pinned Rust toolchain and the MSVC C++ build tools with a Windows SDK, then run `cargo build --release --locked -p victory`. The same launcher finds `target\release\victory.exe`. The sections below describe the upstream AMD fork and its separate launcher.

A fork of [VIctory](https://github.com/Marvisak/VIctory), a compatibility layer
that runs an Xbox Series title natively on Windows from files the user supplies.

## The goal

**A stable 30 fps or more without ray tracing, then as high a frame rate and as
low frame times as the PC allows**, on an AMD RX 7900 XTX first and on PCs with
32 GB (later 16 GB) of RAM.

The way there is **porting, not emulating**: patching the title's renderer so it
behaves the way a PC renderer does, rather than having the layer watch and copy
the console's shared memory every frame. Shaders are translated ahead of time
into a versioned store that is kept between builds. Ray tracing stays optional
and off by default. The picture comes after the frame rate: texture pop-in and
lighting that is too bright or too dark when walking indoors are next in line.

**This repository contains no game files, code or assets.** You need your own
copy of the game. It is not affiliated with or endorsed by the game's developer
or publisher.

## Where it stands (2026-11-26)

Tested on one machine only: Windows 11, Intel i9-12900K, 64 GB RAM, Radeon
RX 7900 XTX, ray tracing off.

| | |
|---|---|
| City street, Series S profile, renderer patches 1, 2, 5, 6 and 7 on | about 45 ms a frame (about 22 fps, 44.9 ms in run merge-5) |
| Same, patches 1, 2, 5 and 6 | 45.6 ms, p95 47 ms |
| Same, patches 1, 2 and 5 | 47.3 ms (about 21 fps), p95 49 ms |
| Series X profile, patches 1+2 | about 53 ms (about 19 fps) |
| Same scene before the patches | 57-75 ms (13-17 fps) |
| Shaders the title uses that translate | all of them (25,790 of 25,985; the rest are unused geometry shaders) |
| Memory | about 30 GB of private memory, 10.5 GB of VRAM: **64 GB of RAM recommended** for now |

It started at 280 ms a frame in the same scenes three days earlier. What sets
the frame now is the one thread that submits the title's work to the card
(about 40 ms); the card itself needs about 36 ms and now waits about 7 ms a frame with nothing queued.

### Renderer patches (off by default while they are tested)

| Switch | What it does |
|---|---|
| `VICTORY_PORT_CONSTANTS=1` | The title's 174 MB constants heap is read by the card where the title writes it: no uploads, no fills |
| `VICTORY_PORT_DESCRIPTORS=1` | The title's descriptor-table stores are patched (byte-checked) to report the slots they write, instead of a scan of ~600,000 slots a frame |
| `VICTORY_PORT_CBV=1` | Translated shaders read constant buffers in place as raw bytes (~22,000 fills a frame to none; -4.7 ms) |
| `VICTORY_PORT_GUARD=1` | The write guards' lock is shared by faults and refreshes, with a claim per 64 KiB block; the submission's planning runs beside it (title threads' waits 29 ms to ~2 ms a frame; -2 ms) |
| `VICTORY_PORT_JOURNAL=1` | The heaps' records are asked for at the title's submit, and the journal's notes are split into 64 parts with locks of their own (the submit thread's waits ~5 ms + 1 ms to ~0.3 ms; -1 to -3 ms) |

### Other switches

| Switch | What it does |
|---|---|
| `--console series-s` | The Series S profile: 1080p output, about 5 ms faster, a softer picture |
| `VICTORY_OUTPUT=1920x1080` | The title renders to that output size itself (Series X profile; about 16 ms less card time) |
| `VICTORY_FRAME_INTERVAL_MS=16.667` | Lets the title go above 30 fps (menus and videos reach 60; the game's clock stays right) |
| `VICTORY_AMBIENT_SETTLE` | How fast light settles after walking indoors (on by default, `250,3`; `0` turns it off) |
| `VICTORY_TEXTURE_STORE=1` | Converted textures kept between runs (about 8 ms a frame for now: off) |

What is known to be wrong:

- Trees, signs and far objects flicker in the distance in some places.
- Lighting is too bright or too dark after walking indoors, until the title has
  run for a while; rooms with little direct light are too dark without `-RayTracing`.
- Textures of what just came into view appear late while the title streams.
- The first run of a new build stutters while the driver makes pipelines
  (`victory warm` makes them ahead).
- Recording a frame in the world makes the title's own watchdog end it.

## How to run it

You need Windows 10/11 x64, a CPU with AVX2, a Direct3D 12 GPU with shader
model 6.6 and mesh shaders, Rust (the toolchain is pinned), the MSVC build
tools, and your own dump of the title.

```powershell
git clone https://github.com/avatarmanz/VIctory-7900XTX
cd VIctory-7900XTX
copy victory.example.toml victory.toml      # then set your paths, outside this folder
cargo build --locked --profile diag -p victory --features diagnostics
scripts\play.ps1 -Exe <path of your copy's executable>
```

`scripts\play.ps1 -Safe` leaves out the newest, faster ways; try it first when
something is wrong. `-RayTracing` turns the title's ray-traced indirect light on.

On a hybrid-graphics PC, the runtime and offline pipeline checks prefer a
high-performance adapter with Shader Model 6.6. The runtime logs its adapter
name, PCI vendor and device IDs, and dedicated VRAM. Set
`VICTORY_D3D12_ADAPTER` to a zero-based DXGI index or part of the adapter's
name to select it explicitly. With the launcher, pass
`-More @{VICTORY_D3D12_ADAPTER='NVIDIA'}`.

Before the first run, ahead of time (both can be stopped and go on later):

```powershell
target\diag\victory.exe shaders <the shader archives>   # every shader translated, minutes
target\diag\victory.exe warm --used                     # pipelines earlier runs drew with, for a new driver or build
```

Commands, debugging switches and upstream's own description:
[docs/README-fxceless.md](docs/README-fxceless.md). Every `VICTORY_...` switch is
described in a comment where it is read.

## What is different here

In short; the reasons and the measurements are in [docs/findings.md](docs/findings.md).

- **List calls are written down, not carried out, on the title's threads**
  (`VICTORY_THIN`): eight threads of ours translate them.
- **Submissions are prepared by a thread of ours and joined**
  (`VICTORY_ASYNC_SUBMIT`, `VICTORY_JOIN_PARTS`), with what can be spread over
  threads spread: refreshes, compares (AVX2), descriptor records, landings.
- **What shaders wrote comes back beside the write guards**, by the dwords that
  changed, on a thread of its own (`VICTORY_RELAY`, `VICTORY_LAND`).
- **Buffer fills that need bytes still on the card are done on the card**
  (`VICTORY_CARD_STEPS`) instead of waiting for it.
- **The title's writes are caught more cheaply**: one view opened at a fault,
  blocks that rest, places in the title's code that look before they write.
- **Textures go up on a copy queue of their own** and a slot shows one only after
  its upload ran, which also ended a device loss that came every 15 minutes or so.
- **Boats float**: textures that shaders write and the title's CPU reads go back
  to title memory (from fxceless and Geckomacabre, on by default here).
- **Frame record and replay** with per-pass times and shader variants compared
  turn by turn ([docs/replay.md](docs/replay.md)).

## Credits

This is a tree of other people's work with more on top. The history is kept, and
commits taken from elsewhere name the commit and its author.

- **[VIctory](https://github.com/Marvisak/VIctory)** by Marvisak and its
  contributors: the loader, the runtime services, the Direct3D 12 back end and
  the shader recompiler.
- **[fxceless/victory-fork](https://github.com/fxceless/victory-fork)**: the fork
  this one started from, and the people whose pull requests it carries.
- **[DexNom/VIctory](https://github.com/DexNom/VIctory)**: ray tracing fixes,
  cube gradients, sparse textures.
- **[Belobug/VIctory-Belobug-AMD](https://github.com/Belobug/VIctory-Belobug-AMD)**:
  the ambient light idea, pipeline workers, and work by romainhedouin.
- **[FQY55kai/ZJOU-VICTORY](https://github.com/FQY55kai/ZJOU-VICTORY)**.
- **[myokex/victory-amd](https://github.com/myokex/victory-amd)**: the guard over
  every view of a page (with Spexivus, joroyt and tightfall), and Joshua Odom's
  reads into title memory.
- **Geckomacabre** and **cxy0072019-sketch**: the texture read-back.
- **[XCL](https://github.com/ivall/xcl)** (GPL-2.0-only) was read for how the same
  title behaves under another layer; no code of it is here.
- This fork's changes were made with [Claude Code](https://claude.com/claude-code).

## License

GPL-3.0-or-later, as VIctory. See [LICENSE](LICENSE); the one file that is not
covered is named in [docs/asset-policy.md](docs/asset-policy.md).
