10 REM Draw Rule 30 in text using the same library as g-cellular-automata.bas.
20 IMPORT "lib/automata.bas" AS Automata
30 DIM Cells(60) : Cells(30)=1 : RuleNo=30
40 PRINT "Rule";RuleNo;" | Neighborhood 100 becomes";Automata.FNCell(1,0,0,RuleNo)
50 FOR Generation=0 TO 30
60   Pattern$=""
70   FOR I=0 TO 60
80     Pattern$=Pattern$+MID$(" #",Cells(I)+1,1)
90   NEXT I
100   PRINT Pattern$
110   IF Generation<30 THEN CALL Automata.NextGeneration(Cells,RuleNo)
120 NEXT Generation
