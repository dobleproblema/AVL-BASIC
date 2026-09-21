# Regresión de rendimiento tras los cambios de ejes y consola — 18 de septiembre de 2026

**Actualización posterior:** se ha incorporado una [optimización de índices limitada a Windows](INDEX-LEAVES-2026-09-18.md). Frente al publicado, la comparación final deja Pi alrededor de un 1,03 % más lento y Jelly prácticamente empatado; la causa original sigue sin identificarse. Las referencias «actual» de este informe corresponden al binario congelado anterior a esa optimización, SHA256 `781310de…`.

**Hay una regresión reproducida en Windows:** el ejecutable actual tarda un 10,40 % más en Pi 10.000 y un 7,18 % más en Jelly sin ventana que el publicado 1.5.96. El cálculo conserva sus salidas y los gráficos su PNG final. Las pruebas aisladas relacionan parte de la diferencia con cambios de disposición del estado y del binario, pero no identifican un mecanismo concreto de CPU o del compilador.

**Conclusión de la fase inicial, antes de la optimización posterior:** regresión abierta, aceptada temporalmente; investigación documentada sin aplicar una optimización. La variante con una única colección de cajas de etiquetas X/Y y funciones alineadas a 64 bytes sólo recupera parte del rendimiento: sigue un 3,25 % más lenta en Pi y un 2,30 % en Jelly en la batería completa. Se descarta para producción; no se modifica código, configuración de compilación ni el ejecutable principal como resultado de esta investigación. Se preservan los cambios funcionales previos de ejes y consola.

## Seguimiento acordado

El 18 de septiembre de 2026 el usuario acordó conservar las mejoras funcionales y observar esta regresión en futuros cambios. Sigue pendiente de resolver. En futuras modificaciones relevantes o versiones para entregar se compararán Pi de 10.000 decimales y Jelly con las referencias conservadas, manteniendo el método de este informe y separando las pruebas con y sin ventana. Se registrará cualquier recuperación o empeoramiento junto con el cambio asociado, sin atribuir un mecanismo no demostrado ni dar por resuelta la pérdida por una medición aislada.

## Referencia y método

Los ejecutables siguientes están identificados por SHA256; las rutas son relativas a la raíz del repositorio Rust:

| Papel | Ejecutable | SHA256 |
| --- | --- | --- |
| Publicado Rust 1.5.96 | `release/avl-basic-1.5.96-windows-x64/avl-basic.exe` | `87471d5e75f4d1049a1ed3fd5b31a5e8863c1e430043424f9fd00def3f7be1b5` |
| Actual, copia fija del principal | `target/perf-regression-20260918/current.exe` | `781310de9bdb74b828ca7d21b8cf1c968586e9adef767fa7a9350a82582886a5` |
| Candidato descartado | `target/perf-regression-20260918/aligned-unified.exe` | `b699819dd1208f7bb257988259bbd5a19e75902aecb1221eac99878155bd9b07` |

Se utilizó [representative.py](representative.py), con programas finitos y tamaños fijados en [representative.json](representative.json): Pi solicita 10.000 decimales; Jelly calcula 10.000 puntos durante 320 fotogramas, con rasterización activa y AVL_BASIC_WINDOW=0. La métrica principal es elapsed_s, tiempo externo del proceso que incluye arranque, carga, salida y PNG. Se alterna AB/BA, rota el orden de las cargas y ejecuta un calentamiento por binario y carga, sin benchmarks ni compilaciones concurrentes.

La comparación inicial tiene seis repeticiones medidas por binario/carga; las pruebas aisladas y la batería completa final, cuatro. Las tablas muestran medianas. Δ = 100 × (tiempo B / tiempo A − 1): positivo significa más lento. Son resultados de esta máquina Windows, no una caracterización de WSL u otras CPU. Un HEAD reconstruido con el entorno actual queda cerca del publicado (+0,40 % Pi y +0,16 % Jelly), por lo que recompilar hoy no explica por sí solo la diferencia completa.

| Carga | Publicado (s) | Actual (s) | Δ actual |
| --- | ---: | ---: | ---: |
| Pi 10.000 | 1,6538 | 1,8257 | +10,40 % |
| Jelly sin ventana | 0,9379 | 1,0052 | +7,18 % |

Todas las ejecuciones válidas mantienen stdout/stderr salvo el marcador de tiempo y el hash del PNG final para las cargas gráficas. La comparación verifica equivalencia con la referencia; no comprueba todos los fotogramas intermedios ni constituye un oráculo matemático independiente. Los JSON conservan las ejecuciones individuales y la dispersión.

La corrección anterior de MERGE sigue intacta: la ejecución ordinaria conserva el préstamo de comandos y avanza mediante `cursor.line_idx + 1`; las búsquedas por número se reservan para los casos de líneas retenidas o fusionadas. Esta investigación no revierte esa corrección.

## Qué cambios se han aislado

| Comparación A → B | Δ Pi | Δ Jelly | Evidencia |
| --- | ---: | ---: | --- |
| Actual → sin repintado nuevo | +0,03 % | +1,07 % | `ablate-repaint/results.json` |
| Actual → sin cambios nuevos de ejes | -4,90 % | -4,03 % | `ablate-axes/results.json` |
| Variante sin ejes nuevos → sólo Vec Y añadido | +4,94 % | +0,93 % | `ablate-size/results.json` |
| HEAD reconstruido → sólo resaltado DATA | +0,08 % | -0,07 % | `ablate-data/results.json` |
| Sólo DATA → repintado, con disposición original de Graphics | +4,33 % | +2,39 % | `ablate-repaint-original-layout-v2/results.json` |

La adición del Vec Y por sí sola cambia el tiempo de Pi aunque este programa no dibuje ejes. Graphics está contenido por valor dentro de Interpreter: cambiar sus campos también cambia los desplazamientos del estado del intérprete. El vector vacío no reserva memoria ni añade trabajo de dibujo al bucle de Pi.

La consola presenta una interacción con el resto del binario: quitar el repintado del actual no recupera Pi, pero incorporarlo sobre la disposición original sí cambia el tiempo. **No se está repintando la consola durante el cálculo.** El flujo main → load_file → run_loaded evita el REPL; normalize_code, apply_identifier_case_for_display y highlight_main no se invocan en los bucles de Pi/Jelly. PRINT y GPRINT tampoco pasan por el resaltador. El cambio aislado de DATA produce diferencias pequeñas. Estos resultados no permiten sumar porcentajes para asignar una contribución independiente a cada cambio.

Se excluye ablate-repaint-original-layout/results.json: los mtimes impidieron incorporar la variante prevista y ambos ejecutables tenían el mismo hash. La tabla utiliza la repetición v2, recompilada correctamente y con binarios distintos.

## Ensamblado y límite de la atribución

La comparación estática usa parejas PE/PDB con GUID y edad verificados. En run_from_inner no crecen el cuerpo ni el número de instrucciones: ambos tienen 20.026 bytes y 3.751 instrucciones; conservan longitudes, secuencia de mnemónicos y destinos relativos de saltos internos. Tras normalizar direcciones absolutas, sus 304 diferencias son desplazamientos de campos incrementados en 24 bytes. Los 57 destinos directos emparejados conservan el tamaño de su rango cuando existe la entrada .pdata correspondiente; dos pares carecen de ella.

La identidad de las funciones privadas se reconstruyó mediante sus llamadores y los límites .pdata; no se dedujo su ausencia por inlining. Los recuentos son estáticos, no recuentos de ejecución. La evidencia demuestra cambios de disposición y descarta crecimiento de ese cuerpo; **no demuestra una causa concreta de caché, predicción de saltos, pila o LLVM**, ni identidad completa de todos los cuerpos llamados.

## Candidato evaluado y descartado

Se ensayó conservar una sola colección de cajas de etiquetas, distinguiendo el eje en cada entrada, junto con alineación de funciones a 64 bytes. La batería completa de ocho cargas, ya conocidas, conserva todas las salidas y PNG. Reduce parte del exceso observado en Pi y Jelly, pero no recupera la referencia:

| Carga | Publicado en esta pareja (s) | Candidato (s) | Δ candidato |
| --- | ---: | ---: | ---: |
| Pi 10.000 | 1,6635 | 1,7176 | +3,25 % |
| Jelly sin ventana | 0,9375 | 0,9591 | +2,30 % |

En las otras seis cargas, la variación va de −0,15 % a +1,25 %. Las comparaciones usan cada una su propia pareja de ejecuciones; no se mezclan medianas de rondas distintas para cuantificar una mejora. La recuperación parcial no justifica introducir este candidato en una petición de diagnóstico. Otras variantes —Graphics en Box, entrada cold y alineación del propio Interpreter— tampoco resolvieron la regresión.

## Comprobación separada con ventana

Cuatro ejecuciones medidas por binario, más un calentamiento, con 10.000 puntos y 1.600 fotogramas. Se alterna AB/BA. Cada fila usa su propia pareja de ejecuciones; no se comparan FPS entre filas.

| Comparación frente al publicado | Publicado (s externos) | Variante (s externos) | Δ tiempo | Publicado (FPS) | Variante (FPS) |
| --- | ---: | ---: | ---: | ---: | ---: |
| Actual | 4,9318 | 5,2685 | +6,83 % | 326,0 | 305,3 |
| Candidato descartado | 4,9841 | 4,9981 | +0,28 % | 322,7 | 321,6 |

Los FPS son 1.600 dividido por la mediana del tiempo BASIC, no lecturas instantáneas del indicador. En todas estas muestras el tiempo BASIC es coherente con el reloj externo; las diferencias corresponden al arranque y salida. Las salidas y PNG finales coinciden. El candidato queda cerca de la referencia en esta prueba de ventana, dentro de su variación, pero sigue sin recuperar completamente Pi. Los datos están en `window-current/results.json` y `window-aligned-unified/results.json`.

## Evidencia y reproducción

La evidencia local se conserva en `target/perf-regression-20260918`:

- `released-vs-current/results.json`: comparación inicial de Pi y Jelly.
- `aligned-unified-full/results.json`: batería final completa del candidato descartado.
- `performance-investigation.md`: tabla ampliada, variantes, rutas y hashes.
- `assembly-comparison.md`: método, identidades PE/PDB, límites y comparación de instrucciones; `avl_pdb_hotspots.py` permite repetir la inspección estática.
- `current.pdb`: copia fija del PDB exacto del ejecutable actual (SHA256 `d410948b4f13ad879865b16dfcc78a9505a3426d3ef5f9ed4c264527e69a1482`). Los ASM, inventarios y diferencias normalizadas se conservan en `assembly/`, además de los temporales originales.
- `identity.json`: HEAD c679447 y hashes de las fuentes al comenzar. Siguen coincidiendo con las fuentes principales; `git diff -- src/main.rs` permanece vacío.

Desde la raíz del repositorio Rust, con los ejecutables conservados y directorios de salida nuevos:

```powershell
python tools/benchmarks/representative.py --baseline release/avl-basic-1.5.96-windows-x64/avl-basic.exe --candidate target/perf-regression-20260918/current.exe --workloads pimachin,jelly --window 0 --runs 6 --warmups 1 --output target/recheck-20260918-current
python tools/benchmarks/representative.py --baseline release/avl-basic-1.5.96-windows-x64/avl-basic.exe --candidate target/perf-regression-20260918/aligned-unified.exe --window 0 --runs 4 --warmups 1 --output target/recheck-20260918-candidate
```

No se han aplicado los candidatos, sustituido el ejecutable principal ni publicado commits por esta investigación. La documentación registra el diagnóstico y sus límites.

## Comparación acotada de ciclos, instrucciones y caché

Se cerró la investigación de contadores tras una comparación básica. No se aplicó ninguna optimización ni se cambió la configuración de seguridad, energía o virtualización. La finalidad es orientar una futura mejora concreta; no se considera justificado seguir construyendo infraestructura para explicar cada efecto microarquitectónico.

### Control sin traza sobre el mismo núcleo P

Intel Core i7-13700K, procesador lógico 2 (P-core), afinidad 0x4. Mismos ejecutables y programas congelados que en la comparación inicial. Cada pareja tiene un calentamiento y cuatro medidas alternadas AB/BA; se han vuelto a comprobar los 20 resultados y hashes de salida/PNG. Jelly usa 320 fotogramas sin ventana.

| Métrica (mediana) | Publicado | Actual | Cambio |
| --- | ---: | ---: | ---: |
| Pi: tiempo externo | 1,693433 s | 1,876152 s | +10,79 % |
| Pi: ciclos contabilizados por Windows | 5.557.075.148 | 6.140.159.316 | +10,49 % |
| Jelly: tiempo externo | 0,957801 s | 1,029284 s | +7,46 % |
| Jelly: ciclos contabilizados por Windows | 3.123.707.007 | 3.351.419.219,5 | +7,29 % |

La regresión persiste en el mismo núcleo P: el simple reparto entre núcleos P/E no la explica. QueryProcessCycleTime es una métrica distinta de los contadores PMU siguientes; no se utiliza para calcular IPC/CPI.

### Captura PMU

WPR, contadores TotalCycles, InstructionRetired y LLCMisses adjuntos a cambios de contexto. Windows requirió elevación administrativa. La primera comprobación elevada se detuvo antes de lanzar pruebas porque Windows registró los contadores en un orden distinto del XML. La captura medida usa el orden real consultado con TraceQueryInformation(TracePmcSessionInformation): [26,29,19], es decir, instrucciones retiradas, fallos LLC y ciclos. No se dedujo el orden por la magnitud de los números.

Una única captura medida contiene los 20 procesos. Lectura completa, cero eventos perdidos, cero intervalos decrecientes y cero discontinuidades de hilo. Hay 1.677 cambios de contexto sin PMC en el conjunto de la traza; ninguno invalida un intervalo de las muestras aceptadas. Se descarta la pareja de Jelly de repetición 1 (índice desde cero): Windows reutilizó el PID 20416 con conhost y el extractor detectó dos ciclos de vida. Quedan cuatro parejas Pi y tres parejas Jelly, sin repetir pruebas para sustituir la muestra descartada.

Los totales siguientes usan exclusivamente los intervalos del procesador lógico 2. En los procesos aceptados, las breves ejecuciones fuera de ese núcleo antes de establecer la afinidad suman como máximo 3,7 microsegundos por proceso. Los contadores incluyen la actividad privilegiada/interrupciones que ocurra durante esos intervalos; no son recuentos exclusivos de instrucciones BASIC ni de modo usuario. Se atribuye cada diferencia entre instantáneas consecutivas del mismo CPU al hilo que sale, comprobando continuidad de hilo y proceso. No se resta entre CPUs.

| Métrica PMU (mediana en LP2) | Pi publicado | Pi actual | Cambio | Jelly publicado | Jelly actual | Cambio |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Instrucciones retiradas | 39.746.156.333 | 39.753.713.210 | +0,019 % | 20.108.727.881 | 20.108.122.614 | -0,003 % |
| Ciclos | 8.651.455.475,5 | 9.536.670.310,5 | +10,23 % | 4.830.308.043 | 5.189.874.265 | +7,44 % |
| Ciclos/instrucción (mediana por proceso) | 0,21766 | 0,23989 | +10,21 % | 0,24021 | 0,25812 | +7,46 % |
| Fallos LLC | 745.841 | 909.203 | +21,90 % | 1.414.869 | 1.406.999 | -0,56 % |

Los fallos LLC tienen una dispersión grande: Pi publicado 498.711-894.347 y actual 658.367-946.429; Jelly publicado 1.399.333-1.671.961 y actual 1.303.335-1.954.970. Los intervalos se solapan. La diferencia de medianas de Pi no demuestra que LLC cause la pérdida de tiempo; Jelly no muestra un aumento comparable. No se midieron L1/L2, predicción de saltos ni esperas internas, y estos datos no excluyen efectos en ellos.

**Conclusión acotada:** la cantidad total de instrucciones retiradas permanece prácticamente constante, mientras aumentan los ciclos por instrucción. Se refuerza que la diferencia no consiste en ejecutar muchas más instrucciones. Sigue sin identificarse el mecanismo físico exacto. No se atribuye un fallo al optimizador de Rust/LLVM. La siguiente vía que se considera más útil es una mejora pequeña en una operación costosa ya identificada que reduzca trabajo medible (búsquedas, copias, accesos indirectos o comprobaciones), seguida de validación de corrección y rendimiento en varias cargas; no una sustitución general de estructuras sin hipótesis comprobable.

Evidencia adicional en `target/perf-regression-20260918/pmc`:

- `p-core-control/results.json`: control sin traza, topología, afinidad y tiempos/ciclos Windows.
- `p-core-hardware-measured/results.json`: ejecuciones, identidades, corrección, comandos y orden registrado.
- `p-core-hardware-measured/registered-counters.json`: respuesta binaria y decodificada de la API de configuración PMC.
- `p-core-hardware-measured/counters.etl`: traza original.
- `p-core-hardware-measured/pmc-totals.json`: totales, intervalos, CPUs, ciclos de vida y guardias de integridad.
- `collect_comparison.py`, `measure_process.py`, `query_pmc_sessions.py`, `ExtractPmcTrace.cs` y `extract-pmc-trace.ps1`: herramientas locales de diagnóstico. WPR confirmó que la sesión propia quedó detenida.

Método de captura basado en la [documentación de Microsoft sobre eventos PMU](https://learn.microsoft.com/en-us/windows-hardware/test/wpt/recording-pmu-events). Esta captura se conserva para orientar decisiones, sin abrir una investigación exhaustiva de caché.
