10 REM Three neighboring cells select one of the rule's eight output bits.
20 DEF FNCell(L,C,R,RuleNo)=INT(RuleNo/2^(4*L+2*C+R)) MOD 2
30 DEF SUB NextGeneration(Cells,RuleNo)
40   LOCAL Scratch(UBOUND(Cells)),I,N
50   N=UBOUND(Cells)+1
60   REM Wrap at the edges and compute the entire next row before copying it back.
70   FOR I=0 TO N-1
80     Scratch(I)=FNCell(Cells((I+N-1) MOD N),Cells(I),Cells((I+1) MOD N),RuleNo)
90   NEXT I
100   FOR I=0 TO N-1
110     Cells(I)=Scratch(I)
120   NEXT I
130 SUBEND
