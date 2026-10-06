# Rust 1.99.0 en Windows y WSL — 6 de octubre de 2026

**Publicación posterior:** la verificación de los binarios definitivos 1.6.11
está en el [informe de publicación](RELEASE-1.6.11.md). Las cifras y los hashes
de este estudio corresponden a 1.6.10 recompilado antes de esa publicación.

Se actualizan ambos entornos a Rust/Cargo 1.99.0, con LLVM 23.1.1, y se fija
la versión compartida en [`rust-toolchain.toml`](../../rust-toolchain.toml).
Se conserva el intérprete 1.6.10 original y su plan opcional para expresiones
de arrays en ambos sistemas. La mitigación que separaba ese plan del
despachador se vuelve a medir con el nuevo compilador y se descarta: ahora
empeora Pi y Jelly en Windows y las tres cargas comprobadas en WSL.

La actualización reduce el tiempo de Pi y mejora modestamente Jelly, pero
no acredita una recuperación completa respecto a 1.5.96. Tampoco es una
mejora universal: Mandelbrot empeora en Windows.

## Actualización y configuración

La versión estable [1.99.0 se publicó el 1 de octubre de 2026](https://blog.rust-lang.org/2026/10/01/Rust-1.99.0/).
Se ejecutó `rustup update stable` en Windows y WSL. Ambos `rustc -Vv`
muestran el commit `b940084d7eb6a299eb4bfeb8e34901bc051e7ac4`, LLVM 23.1.1
y Cargo 1.99.0. Los targets nativos son Windows MSVC x64 y Linux GNU x64.

Se instala además el toolchain versionado 1.99.0 con perfil mínimo y rustfmt,
y se eliminan los overrides del proyecto: el de Windows y las dos grafías
WSL `Documents/Rust/AVL-BASIC` y `Documents/rust/AVL-BASIC`. Ambas pasan a
respetar el archivo compartido. Los toolchains anteriores se conservan
instalados para reproducir comparaciones históricas. No se cambian MSVC,
los enlazadores nativos, el perfil release ni las dependencias.

Para verificar desde la raíz del repositorio en cualquiera de los sistemas:

```sh
rustup show active-toolchain
rustc -Vv
cargo -V
```

## Efecto de actualizar sólo Rust

Fuente: `505974764e93a8aff222d38ecc69d71c7d104005` (1.6.10), sin cambios del runtime,
`Cargo.toml` o `Cargo.lock`. Los controles Windows y WSL son los ejecutables
anteriores verificados por sus hashes. Cuatro muestras por ejecutable y
carga, más un calentamiento; mismo programa congelado y afinidad a CPU
lógica 2. Se alterna AB/BA y se ejecuta cada comparación secuencialmente.

La tabla da medianas del tiempo total del proceso, en segundos. Δ negativo
significa menos tiempo. La MAD y todas las muestras están en el JSON.

| Carga | Windows 1.95 → 1.99 (s) | Δ tiempo | WSL 1.98.1 → 1.99 (s) | Δ tiempo |
| --- | ---: | ---: | ---: | ---: |
| Raytracer | 2,306 → 2,284 | -0,94 % | 2,302 → 2,233 | -2,98 % |
| Pi 10.000 | 1,806 → 1,712 | -5,22 % | 2,221 → 2,119 | -4,56 % |
| N-Queens | 1,252 → 1,258 | +0,52 % | 1,344 → 1,287 | -4,25 % |
| Jelly sin ventana | 0,997 → 0,988 | -0,99 % | 0,984 → 0,970 | -1,39 % |
| Mandelbrot | 0,656 → 0,677 | +3,17 % | 0,639 → 0,610 | -4,56 % |
| Cadenas | 1,366 → 1,330 | -2,63 % | 0,835 → 0,836 | +0,13 % |
| Pi moderno 10.000 | 2,114 → 1,978 | -6,39 % | 2,547 → 2,390 | -6,19 % |
| Matrices | 0,173 → 0,173 | -0,33 % | 0,113 → 0,112 | -1,23 % |
| Smoke 480 frames | 5,518 → 5,491 | -0,48 % | 5,441 → 5,376 | -1,20 % |

Media geométrica del tiempo de las nueve cargas:

- Windows: 1,52 % menos tiempo.
- WSL: 2,93 % menos tiempo.

## Jelly con ventana real

Comprobación Windows separada: 10.000 puntos y 1.600 frames, cuatro muestras
por ejecutable y un calentamiento. Métrica `body_s` (`TIME` del BASIC),
afinidad a CPU lógica 2 y ventana real (`AVL_BASIC_WINDOW=1`).

La mediana pasa de 5,1763 a 5,0864 s:
**309,1 → 314,6 FPS**, un 1,74 % menos tiempo.
Es una mejora frente al control de esta misma sesión; los ~325 FPS históricos
no se han recuperado. Las imágenes finales coinciden.

## Mitigación con el mismo compilador en ambos sistemas

Se recompila la misma separación mediante `eval_specialized_array_rhs` y
`#[inline(never)]` en ambos sistemas con Rust 1.99.0. El plan y su fallback
siguen activos. El parche experimental queda en
[`array-rhs-outlined-2026-10-06.patch`](array-rhs-outlined-2026-10-06.patch); **no está aplicado**.

Se compara contra el código original compilado también con 1.99.0, desde
la misma ruta del proyecto y con el mismo perfil. Seis muestras por
ejecutable y carga, más un calentamiento; medianas del tiempo de proceso.

| Carga | Windows original → separado (s) | Δ tiempo | WSL original → separado (s) | Δ tiempo |
| --- | ---: | ---: | ---: | ---: |
| Pi 10.000 | 1,706 → 1,751 | +2,58 % | 2,112 → 2,192 | +3,81 % |
| Jelly sin ventana | 0,990 → 1,002 | +1,26 % | 0,961 → 0,997 | +3,82 % |
| Smoke 480 frames | 5,493 → 5,508 | +0,28 % | 5,373 → 5,655 | +5,25 % |

El beneficio que presentaba en Windows con 1.95 no se mantiene con 1.99.
Se conserva el código original: no se añade una selección del plan por
plataforma ni se ignora su evaluación. Estas medidas no demuestran un fallo
de corrección del compilador ni identifican un mecanismo físico de caché.

Con 1.99, la sección Windows `.avlrun` del original mide 20.084 bytes y la
del candidato 19.975; `.avleval` mide 7.689 en ambos. Las secciones siguen
siendo RX, alineadas a 4 KiB y con datos de unwind válidos. Un despachador
algo más pequeño no garantiza que se ejecute más rápido.

## Comprobación de la recompilación final

Después de retirar el experimento se recompila el código original con el
toolchain fijado. Linux reproduce exactamente el ELF medido. Windows genera
otro SHA-256 (`6fb96072adf5f77a8e1e71a0e437279f5f1c493700c5249ba536d5b04908f682`):
las dos funciones dedicadas mantienen tamaño y ubicación, pero `.rdata`
se reduce 512 bytes y cambian las direcciones de algunas secciones y las
referencias en el código. No se presupone igualdad del rendimiento.

Se comprueban Pi, Jelly sin ventana y Smoke con seis muestras por ejecutable
y un calentamiento, comparando los dos originales 1.99.0:

| Carga | Primer binario → recompilación (s) | Δ tiempo |
| --- | ---: | ---: |
| Pi 10.000 | 1,7090 → 1,7095 | +0,03 % |
| Jelly sin ventana | 0,9889 → 0,9929 | +0,40 % |
| Smoke 480 frames | 5,5413 → 5,5255 | -0,29 % |

Las diferencias quedan por debajo del 0,5 %. Las salidas e imágenes
coinciden. No aparece una regresión comparable a la original. Para que la
entrega coincida exactamente con la comparación amplia y la prueba de
ventana, se dejan en las rutas habituales los binarios 1.99.0 medidos en
esas pruebas, con el PDB correspondiente. La recompilación adicional y su
PDB se conservan aparte con todos sus resultados.

## Validación y binarios locales

- Código original con Rust 1.99.0: **957 pruebas Windows aprobadas**
  (8 ignoradas) y **969 WSL aprobadas** (1 ignorada); cero fallos.
- `cargo fmt --all -- --check` pasa. Las pruebas completas se ejecutan
  sobre el runtime original; el candidato descartado se valida mediante
  las tres comparaciones de rendimiento y sus hashes de salida e imagen.
- Todas las comparaciones comprueban la igualdad de stdout normalizado y
  PNG: las 316 ejecuciones, incluidos los calentamientos, pasan.
- Los binarios entregados son exactamente los originales 1.99 medidos
  en las comparaciones amplias. Sus hashes y el PDB se verifican de nuevo.
- La compilación genera un aviso nuevo de deprecación de `fetch_update`
  en `src/audio/shared_cpc.rs`; sigue compilando correctamente. No se
  modifica esa llamada durante la comparación del compilador.

Los ejecutables de trabajo siguen siendo 1.6.10; no se crea una publicación
ni se cambia la versión del intérprete. El repositorio Python no cambia.

| Binario final | SHA-256 |
| --- | --- |
| `target/release/avl-basic.exe` | `2512a70cbae6ab34b0cbf692ff4953e4dd8eb4da9c42344308eea77108dc06fe` |
| `target/release/avl-basic` | `4f025b8a3765a1a786254a1f44cf3700f71db307b5725103640cc2299012d875` |

El PDB Windows corresponde al mismo EXE. Los ejecutables y los programas
congelados se conservan localmente bajo
`target/perf-regression-20261006/compiler-1.99.0/`, que está ignorado en Git.
Un clon nuevo requiere recuperar esos artefactos y comprobar sus hashes.

La evidencia versionable está en [`toolchain-results-2026-10-06.json`](toolchain-results-2026-10-06.json):
versiones nativas, fuentes y sus hashes, configuraciones, muestras, medianas,
MAD, hashes de corrección y comprobaciones de las secciones del EXE.

Para evitar repetir los 10.000 dígitos de Pi en cada muestra, el JSON
guarda una salida normalizada por hash de corrección y el hash de stdout
de cada ejecución. Los stdout completos siguen en los resultados locales.
