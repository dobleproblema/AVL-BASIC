# Verificación de la publicación 1.6.11 — 6 de octubre de 2026

Los binarios definitivos conservan aproximadamente el rendimiento de la
configuración elegida con Rust 1.99.0. Las pequeñas diferencias de esta
recompilación quedan registradas; no se atribuyen automáticamente a ruido.

La comparación anterior del [cambio de compilador](TOOLCHAIN-2026-10-06.md)
acredita una recuperación parcial. La regresión histórica respecto a 1.5.96
continúa incompletamente explicada y no se presenta como resuelta.

## Qué se compara

- Referencia: 1.6.10 recompilado con Rust 1.99.0 y conservado al cerrar el estudio.
- Candidato: los binarios definitivos 1.6.11, compilados con el mismo Rust/Cargo
  1.99.0 y LLVM 23.1.1 en Windows MSVC x64 y WSL Linux GNU x64.
- El código del intérprete coincide con el publicado 1.6.10. Se incrementa la
  versión y se fija el compilador común en rust-toolchain.toml. El plan opcional
  de expresiones de arrays sigue activo en ambos sistemas.
- Un calentamiento y cuatro muestras medidas por ejecutable y carga, orden
  alternado AB/BA, programas congelados y afinidad a CPU lógica 2 dentro de cada
  sistema. No se asume que identifique el mismo núcleo físico en Windows y WSL.
- 70 ejecuciones incluyendo calentamientos: 30 Windows sin ventana, 30 WSL sin
  ventana y 10 Windows con ventana real. Sin compilaciones o pruebas simultáneas.

Pi calcula 10.000 dígitos; Jelly usa 10.000 puntos y 320 frames sin ventana;
Smoke usa 480 frames, calidad MAX y SMOOTH ON. Las tablas dan medianas del
tiempo externo del proceso y MAD en segundos. Cambio positivo significa
mayor tiempo. Se comprueban stdout/stderr normalizados y el PNG final.
No se verifican todos los fotogramas intermedios.

## Windows

| Carga | 1.6.10 local | 1.6.11 final | Cambio | MAD base / final |
| --- | ---: | ---: | ---: | ---: |
| Pi, 10.000 dígitos | 1,706174 | 1,708403 | +0,13 % | 0,000912 / 0,001023 |
| Jelly sin ventana | 0,987625 | 0,989735 | +0,21 % | 0,002329 / 0,003450 |
| Smoke, 480 frames | 5,475669 | 5,542327 | +1,22 % | 0,004183 / 0,006332 |

## WSL

| Carga | 1.6.10 local | 1.6.11 final | Cambio | MAD base / final |
| --- | ---: | ---: | ---: | ---: |
| Pi, 10.000 dígitos | 2,113444 | 2,111168 | -0,11 % | 0,006129 / 0,004593 |
| Jelly sin ventana | 0,975648 | 0,951088 | -2,52 % | 0,003810 / 0,000367 |
| Smoke, 480 frames | 5,381004 | 5,419323 | +0,71 % | 0,004574 / 0,021256 |

## Jelly con ventana real en Windows

10.000 puntos y 1.600 frames. Mediana de body_s, medida con TIME dentro del BASIC:
5,108898 → 5,107055 s,
**313,2 → 313,3 FPS**.
MAD: 0,014581 / 0,014144 s.
Cada tiempo interno es positivo y menor que el tiempo externo. Los PNG finales
coinciden. No se midió Jelly con ventana en WSL ni se recuperan los ~325 FPS
históricos como resultado de esta publicación.

## Corrección y entrega

- Rust: 957 pruebas Windows y 969 WSL superadas, cero fallos; ocho y una
  ignoradas respectivamente. Las siete pruebas Windows que requieren teclado
  y foco reales no forman parte de esta tanda; se validaron en 1.6.10.
- Python de referencia: 1.367 pruebas superadas. Su repositorio permanece
  en 1.5.83, sin cambios.
- Paridad: 392 programas de texto (389 con oráculo Python y tres extensiones
  Rust), 103 sesiones directas, 54 comprobaciones gráficas y tres sesiones
  gráficas directas. 90 comprobaciones de subarrays y 174 de expresiones MAT.
- Manuales empaquetados: 66/66 escenarios superados en Windows y 66/66 en WSL,
  correspondientes a 56 ejemplos publicados EN/ES, sin ventana y con audio
  silencioso. Las adaptaciones de entrada y espera constan en los registros.
- Formato, sincronización de 270 temas y 64 errores, y catálogo de 131 ejemplos:
  correctos. Las secciones Windows .avlrun y .avleval conservan alineación
  a 4 KiB, permisos RX y metadatos de desenrollado válidos.

Los paquetes se generan con --skip-build a partir de estos mismos binarios.
La auditoría de archivos, versiones, muestras, documentación y hashes se
conserva localmente en target/release-validation-1.6.11/. El manifiesto
avl-basic-1.6.11-SHA256SUMS.txt acompaña a los paquetes de la publicación.

| Binario definitivo | SHA-256 |
| --- | --- |
| Windows | `9c38031804a68fa0eb3286eab3fcfbac1bbb715849ad8344ce297f363270185f` |
| WSL/Linux | `0021327833ae3441430df242ddbc2a0d48b216dc2c92669e222a21214b927a4a` |

El [JSON permanente](release-1.6.11-results.json) conserva las 70 ejecuciones, medianas y MAD,
hashes de ejecutables y fuentes, programas congelados y salidas normalizadas
sin duplicación. Los stdout originales y los registros completos permanecen
en la carpeta local de validación. Esta comparación mide estabilidad de la
publicación elegida; no constituye una nueva búsqueda de optimizaciones.

## Auditoría de paquetes

Los ejecutables empaquetados imprimen 1.6.11 y resuelven PRINT 6*7 con 42.
Cada archivo de las 273 muestras y recursos coincide con su fuente. Hay
131 programas, 20 destacados, 11 imágenes incrustadas en README.html y 187
enlaces HTML locales verificados en cada paquete. Los tres documentos HTML
pasan la regeneración y comprobación de contenido. El archivo Linux conserva
permisos 0755 para el ejecutable y los directorios e incluye lanzador e iconos;
los archivos de integración usan LF y requiere glibc 2.39 o posterior. El instalador shell se
invoca con sh y se distribuye con permisos 0644.

| Archivo | SHA-256 |
| --- | --- |
| avl-basic-1.6.11-windows-x64.zip | `8c607b725f01328484a895b43ba0d8cc5275d99b19f857da0c9f4f4a2b7f687d` |
| avl-basic-1.6.11-linux-x64.tar.gz | `ad7dd5df0d0dc134c83d2e098aeca16686ef2d576be4fdd0a1a63c8ec0ca48dc` |
