# CellHub

Запуск на macOS: дважды нажмите `start.command` в Finder.
Или выполните `./start.command` в терминале из папки проекта.
Скрипт соберёт проект и запустит сервер. Откройте http://localhost:8080.
Остановка: Ctrl+C в окне терминала.

Минимальный C++17 → WebAssembly проект с отрисовкой цветного квада через WebGL 2.
Четыре вершины, индексный буфер из шести индексов, GLSL ES 3.00 шейдеры.
Вся отрисовка находится в `src/main.cpp`; JavaScript в HTML отвечает только за загрузку и сообщения.
Размер canvas учитывает плотность пикселей, квадрат сохраняет пропорции при изменении окна.

## Подготовка

Нужны CMake 3.20+, Python 3 и [Emscripten SDK](https://emscripten.org/docs/getting_started/downloads.html).
Установите SDK один раз из папки проекта:

```sh
git clone https://github.com/emscripten-core/emsdk.git .tools/emsdk
cd .tools/emsdk
./emsdk install latest
./emsdk activate latest
cd ../..
```

Скрипт сборки автоматически подключает локальный SDK из `.tools/emsdk`.
Если SDK установлен в другом месте, выполните `source /path/to/emsdk/emsdk_env.sh`.

## Сборка и запуск

Из папки проекта:

```sh
./start.command
```

Откройте http://localhost:8080. Требуется браузер с WebGL 2 и WebAssembly.
Не открывайте HTML через `file://`: загрузка WASM требует HTTP.

Результат сборки: `build/index.html`, `build/index.js`, `build/index.wasm`.
Для размещения на сервере скопируйте все три файла; MIME-тип `.wasm` — `application/wasm`.

Флаги WebGL соответствуют [документации Emscripten](https://emscripten.org/docs/porting/multimedia_and_graphics/OpenGL-support.html).
