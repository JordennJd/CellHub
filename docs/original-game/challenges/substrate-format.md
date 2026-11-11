# Формат файлов `.substrate` (Cell Lab v103)

Дата: **11 ноября 2026**. Источник: обратная разработка `cell_lab_103_android_patched.apk` (package `com.saterskog.cell_lab`, чистая Java, без нативного кода). Порядок чтения ниже дословно следует декомпилированным методам `Environment.a(ObjectInputStream)`, `Cell.a(ObjectInputStream)`, `Gene.a(ObjectInputStream, int)` и `Link.a(ObjectInputStream, int)`; ветки версий проверены по smali там, где jadx не справился.

Эталонный декодер — `scripts/extract_cell_lab_challenges.py`; его вывод лежит в `data/<id>.json`.

## Общая структура файла

Файл — Java Object Serialization Stream (`AC ED 00 05`). Примитивы пишутся блоками `TC_BLOCKDATA` (`0x77` + длина-байт, `0x7A` + длина-int32); чтение примитивов переходит через границы блоков прозрачно.

```
int32   fileVersion            (у шиппед-файлов 5…95; читатель принимает ≤ 95)
double  M                      начальное время симуляции, часы (обычно 0…26)
int32   z                      число клеток
Environment                    (см. ниже; пишет свою версию ≤ 95)
если fileVersion > 12:
    GZIP-поток, внутри которого новый ObjectOutputStream (снова `AC ED 00 05`)
если fileVersion > 23:
    double O                   начальная фаза вращения света
z × Cell
int32   B                      число частиц еды
B × Food
```

**Особенность шиппед-файлов:** gzip-поток обрезан — без завершающего маркера и CRC. Игра не читает поток до конца (ровно `z` клеток и `B` частиц), поэтому декомпрессор должен отдавать данные инкрементально и не требовать трейлер.

## Environment (настройки субстрата)

Все `double` — big-endian float64, `float` — float32.

```
int32   envVersion
double  food                   (env v1: значение × 0.15)
double  foodSize               (env v ≤ 1: нет, считать 0.15)
double  radiation
double  light
double  lightRotationSpeed     свет вращается: O += speed·dt
double  lightRange
если envVersion ≤ 23: double (устаревший слот между light_range и viscosity)
double  viscosity
int32   storedTypeCount        сколько типов знала записавшая версия
storedTypeCount × bool       t[] — разрешённые типы (порядок — enum h, см. README)
bool    w                      addRandomCells
double  i                      gravity
double  h                      density (отрицательная = клетки легче воды)
double  j                      gradient
bool    x                      dieAtEdge
если envVersion > 20: double z (nitrates 0…1)
иначе: bool → 1.0/0.0
если envVersion ≥ 3:
    int32 r                    maxCells (default 800)
    int32 s                    устаревший слот (3500; в v103 не используется)
    double k                   substrateRadius (координаты нормированы ±k)
иначе: r=800, s=3500, k=1.0
если envVersion ≥ 9:
    double m                   friction (динамическое)
    double l                   friction (статическое)
    bool   u                   tweak
иначе: 0, 0, false
если envVersion ≥ 10: float  n   salinity; иначе 0.3
если envVersion ≥ 14:
    bool   v                   aging
    double p                   foodLump (0…100)
    double q                   foodLumpSize (0.01…0.2)
иначе: true, 0.0, 0.01
если envVersion ≥ 22:
    bool   y                   mobileFood
    float  o                   foodLayer
иначе: false, 0.0
```

## Cell

```
int32   cellVersion            (2…95; у шиппед-файлов обычно совпадает с fileVersion)
double  b, c                   позиция
double  k                      угол
double  m                      поворот (rotation)
double  e, f                   скорость
double  l                      угловая скорость
double  n                      угловая позиция
double  d                      радиус
double  w                      масса
double  o                      возраст, часы
int32   linkCount
min(linkCount,20) × Link      (лишние читаются и отбрасываются)
int32   C                      таймер режима (кадры)
bool    x                      мёртвая
float   J[3]                   цвет RGB (текущего режима)
int32   geneCount
min(geneCount,40) × Gene      (лишние читаются и отбрасываются)
                               недостающие слоты 40-массива = копия I[0] с p=false;
                               затем первый помеченный p=true становится основным
int32   D                      основной режим (0…39)
int32   E                      класс владельца: 0 уровень, 1 игрок, 3 заражённая, 6 гибрид
int32   F                      состояние
double  t, u, v, p             рабочие поля физики
если cellVersion ≥ 4: bool y
если cellVersion ≥ 9:
    float U[4]                 поглощение субстрата (default 0.01)
    float W[4]                 выделение в субстрат (default 0.01)
    float X, Y, aa, ab, ac     рабочие поля
    если cellVersion > 11: float S; иначе 0
если cellVersion ≥ 15: int32 G
если cellVersion ≥ 19: int32 T (метка); иначе −1
если cellVersion ≥ 23: double q; иначе 0
```

## Gene (один режим генома)

```
если cellVersion > 2: int32 geneVersion; иначе geneVersion = 1
```

### Ветка geneVersion < 7 (старые файлы; все углы потом нормируются mod 2π)

```
float   a[3]                   цвет RGB
double  b                      splitMass
double  c                      child2MassRatio (ребёнок 1 = 1 − c)
double  d                      splitAngle (geneVersion<9: + π)
double  e                      child1Angle (geneVersion==5: + π; geneVersion<9: ещё + π)
double  f                      child2Angle (geneVersion<9: + π)
double  g                      приоритет, линейная шкала (в новых версиях log10)
int32   i                      child1Mode
int32   j                      child2Mode
bool    l                      makeAdhesin
bool    m                      keepAdhesinChild1
bool    n                      keepAdhesinChild2
int32   s                      тип клетки (порядок enum h)
int32   k                      устаревшее
bool    o                      stayAlive
double  f4                     коэфф. миграции → канал 1
если geneVersion ≥ 3: int32 i7, i6  (→ выходы u[0], u[1])
если geneVersion ≥ 4:
    bool p, q, r               isPrimary, mirrorChild1, mirrorChild2
    double h                   жёсткость (без ×35!)
```

### Ветка geneVersion ≥ 7 (современная)

```
float   a[3]                   цвет RGB
float   b, c, d, e, f          splitMass, child2MassRatio, splitAngle, child1Angle, child2Angle
                               (geneVersion<9: d,e,f += π; c = 1 − c)
float   g                      приоритет (log10, клампится 0.1…10)
int32   i, j                   child1Mode, child2Mode
bool    l, m, n                makeAdhesin, keepAdhesinChild1, keepAdhesinChild2
int32   s                      тип клетки
int32   k                      устаревшее
bool    o                      stayAlive
если geneVersion < 9: float f (коэфф. миграции → канал 1), int32, int32 (→ u[0], u[1])
bool    p                      isPrimary
bool    q                      mirrorChild1
bool    r                      mirrorChild2
если geneVersion < 16: float h × 35 = жёсткость (0…35); иначе float h
если geneVersion == 8: float f3 (коэфф. миграции → канал 0); geneVersion<8: 0.4
```

### Каналы сигнализации (geneVersion ≥ 9)

11 каналов S1…S4/S5…S8 (8 веществ + 4 «виртуальных» −S1…−S4/+S1…+S4 в редакторе):

```
11 × {
    int16 d                    вещество
    если geneVersion ≥ 17: int16 e (функция переноса)
    иначе e выводится из d: −1→(0,нет); <4→1; ≥4→d−4, 2
    float a, b, c              коэффициенты (клампятся к ±8)
}
11 × int32 u[i]              выбранное назначение выхода канала (границы w[i]=[40,40,4,6,15,4,4,4,4,20,6])
7  × float  v[i]              типоспецифичные параметры (диапазоны z/A:
                               v0 ±60, v1…v3 0…1, v4 0…√3, v5 0…0.01, v6 0.5…2;
                               для geneVersion<14 v6=1.0; для geneVersion<16 v5=0.0)
```

Особые значения при чтении: `u[4]` = 1 если geneVersion==10; `u[9]` = 18 если geneVersion<15; `u[10]` = 0 если geneVersion<20.

**Миграция старых файлов (geneVersion < 9, на диске каналов нет):** `t[0].b = 2·f3 − 1`, `t[1].b = 2·f4 − 1` (нормировка x[0]=0, y[0]=1), `v[6] = 1.0`, `u[0] = i7`, `u[1] = i6`, `u[9] = 19`; остальные каналы обнулены. Декодер записывает эти вычисленные значения в JSON с пометкой `migrated_from_legacy`.

## Link (связь с соседней клеткой)

```
int32   a                      индекс соседа в массиве клеток
double  c, d                   позиция стыка
bool    b                      связь разорвана
double  e, f                   рабочее
если cellVersion ≥ 7: float  k (жёсткость); 4…6: double; иначе 0
если cellVersion ≥ 16: float l (коэфф. длины); иначе 0
```

## Food (частица еды)

```
если fileVersion ≥ 8: float a, b, c (позиция); иначе double
если fileVersion ≥ 22: float d, e, f (начальная скорость; иначе 0)
```

## Границы реконструкции

- Назначения слотов `u[]`/`v[]` для конкретных типов клеток (секреция Secrocyte, smell Senseocyte, выходы Neurocyte, скорость Ciliocyte, теломеры Gamete и т.д.) известны из кода редактора как списки опций, но в этой выгрузке не атрибутированы поиндексно: в JSON они хранятся как массивы `outputs`/`type_params` без переименования.
- Рабочие (не сохраняемые игрой) поля — `Cell.O` (случайный seed), текущие скорости частиц после старта — в файлах отсутствуют по определению формата.
