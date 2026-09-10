10 ' CSV preserves quotes, commas, newlines and numbers. Replaces f-records.csv.
20 Q$=CHR$(34)
30 S$="Hello, "+Q$+"reader"+Q$+CHR$(13)+CHR$(10)+"Second line"
40 X=1/7
50 OPEN "f-records.csv" FOR OUTPUT AS #1
60 WRITE #1,S$,X,""
70 CLOSE #1
80 OPEN "f-records.csv" FOR INPUT AS #1
90 INPUT #1,T$,Y,E$
100 PRINT "Text preserved: ";S$=T$
110 PRINT "Number preserved: ";X=Y
120 PRINT "Empty field preserved: ";LEN(E$)=0
130 PRINT "End of file: ";EOF(1)
140 CLOSE
