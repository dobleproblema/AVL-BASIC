# Medición del rendimiento del intérprete

Informes en español:

- [Primera ronda: 4 de septiembre de 2026](RESULTS-2026-09-04.md).
- [Diagnóstico con perfiles y decisión: 5 de septiembre de 2026](PROFILING-2026-09-05.md).
- [Regresión de MERGE y corrección: 11 de septiembre de 2026](REGRESSION-2026-09-11.md).

Los candidatos de estas rondas fueron descartados y sus parches experimentales
retirados. Se conservan las mediciones, conclusiones y herramientas de diagnóstico.

Las herramientas Python requieren Python 3.10 o posterior.

## Comparaciones de ejecutables

`representative.py` compara ejecutables release existentes. No compila ni cambia
los ejemplos originales. Utilizar el mismo compilador, perfil, máquina y
condiciones para ambos binarios, sin otras compilaciones ni mediciones en paralelo.
Los comandos siguientes parten de la raíz del repositorio Rust.

```powershell
python tools/benchmarks/representative.py --baseline C:/bench/baseline.exe --candidate target/release/avl-basic.exe --output C:/bench/comparison --runs 6 --warmups 1
```

Con solo `--baseline` se calibran las duraciones. `--prepare-only` genera los
programas adaptados sin ejecutarlos. `--workloads jelly,strings` selecciona casos.
El JSON contiguo fija tamaños y grupos; no debe cambiarse según las ganancias de
un candidato. `--group tuning` selecciona las seis cargas principales. Los casos
`modern` y `matrices`, etiquetados históricamente como `validation`, ya se han
consultado: sirven para regresión, pero dejaron de ser validación inédita.

Los gráficos conservan su rasterización con `AVL_BASIC_WINDOW=0`. No hay ventana,
entrada interactiva, sincronización de pantalla ni limitación de fotogramas.
Raytracer usa 640 × 480; Jelly, 10.000 puntos y 320 fotogramas completos;
Mandelbrot, una vista de 640 × 480 y hasta 80 iteraciones. Machin solicita 10.000
dígitos y N-Queens repite 16 veces la búsqueda original N=16. La carga textual
procesa registros con arrays de strings, normalización, fragmentos, búsqueda,
concatenación y conversión numérica. No representa todos los programas de texto.

Se alterna el orden base/candidato, rotando también el orden de las cargas.
Cada ejecución debe conservar stdout y stderr salvo el marcador explícito de
tiempo; los PNG finales también deben coincidir. Esto comprueba equivalencia
con la referencia, no la corrección matemática independiente ni todos los
fotogramas intermedios. El JSON guarda hashes, tiempos individuales, salidas,
medianas y desviación absoluta mediana (MAD).

La métrica principal `elapsed_s` usa el reloj monotónico `time.perf_counter()`
alrededor del proceso: incluye arranque, carga, interpretación, salida y PNG final.
`body_s` utiliza `TIME` dentro del BASIC antes de exportar el PNG. Este segundo
reloj es de sistema y puede saltar: en el diagnóstico del 5 de septiembre se
detectaron cuatro valores incompatibles con el tiempo externo. Conservarlos
como evidencia, pero excluirlos del análisis de `body_s`. Las comparaciones
publicadas utilizan `elapsed_s`.

`--metric body_s` cambia explícitamente la métrica; `--window 1` permite una
comprobación separada con ventana real. No mezclar esos resultados con las
comparaciones sin ventana. Calibrar con la referencia cargas de aproximadamente
0,5–1 segundo o más antes de probar candidatos; no ajustar su tamaño después.

Una reducción positiva del tiempo es `100 * (1 - mediana_candidato / mediana_base)`;
no es el mismo porcentaje que el aumento del rendimiento por segundo.
Los agregados usan medias geométricas de razones de tiempo con igual peso por
carga. Publicar también cada caso para que una media no oculte regresiones.
Cambios semánticos requieren pruebas pertinentes además de los benchmarks.

## Casos reservados y oráculos

`heldout-2026-09-05.json` fija potencias enteras (`7^10000`) y edición/búsqueda
Unicode con cuatro longitudes de buffer y 30.000 pasadas. Se calibraron solo con
la referencia; ningún candidato de la segunda ronda llegó a medirse con ellos.

```powershell
python tools/benchmarks/representative.py --baseline C:/bench/baseline.exe --config tools/benchmarks/heldout-2026-09-05.json --output C:/bench/heldout-baseline --runs 1 --warmups 0
python tools/benchmarks/verify_heldout.py C:/bench/heldout-baseline/results.json
```

El verificador usa Python para calcular de forma independiente el entero
completo y los textos, cambios y checksum Unicode esperados. Consultar estos
casos para dirigir una optimización los convertiría en casos de ajuste.

## Perfiles de CPU

`profile_linux.py` toma muestras con `perf`, pilas DWARF y un ejecutable optimizado
con símbolos. Conserva los datos originales, porcentajes exclusivos e inclusivos,
calidad de las pilas y equivalencia frente a una referencia sin perfilador.
En WSL, usar un directorio de salida nuevo dentro de su sistema de archivos Linux.

`profile_windows.py` utiliza VSDiagnostics de Visual Studio. Inicia el intérprete
suspendido, adjunta la captura al PID exacto, reanuda y cierra la sesión al terminar,
también ante errores. `extract-cpu-trace.ps1` y `ExtractCpuTrace.cs` leen ETW con
TraceEvent y PDB locales exactos; requieren PowerShell 7 y, con la configuración
actual del wrapper, Visual Studio 2022 Community instalado en su ruta estándar
`C:\Program Files\Microsoft Visual Studio\2022\Community`. El extractor busca sus
dependencias en `Common7\IDE\PrivateAssemblies`: cambiar `--collector` no modifica
esa ruta. Los símbolos públicos de DLL del sistema permiten reducir la atribución
desconocida. Consultar `--help` y los comandos del informe para rutas y requisitos
concretos.

Los porcentajes corresponden a funciones físicas: una función puede contener
otras integradas por el compilador. No llamar despacho puro a `run_from_inner`,
no sumar porcentajes inclusivos de niveles anidados y no convertir muestras de
CPU en ahorro potencial. Medir cualquier candidato después con ejecutables
release normales y sin perfilador; añadir símbolos puede cambiar el código.
