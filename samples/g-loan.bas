100 DEF SUB SHOWHEADER
110   GPRINT "===================================="
120   GPRINT "   BASIC LOAN CALCULATOR"
130   GPRINT "   Classic + modern features"
140   GPRINT "===================================="
150   'GPRINT
160 SUBEND
170 '
180 DEF FNPAYMENT(principal, annualRate, years)
185   LOCAL months, r, factor
190   months = years * 12
200   IF annualRate = 0 THEN
210     FNPAYMENT = principal / months
220     EXIT FN
230   END IF
240   r = annualRate / 100 / 12
250   factor = (1 + r) ^ months
260   FNPAYMENT = principal * (r * factor) / (factor - 1)
270 FNEND
280 '
290 DEF SUB YEAR1TABLE(principal, annualRate, years)
295   LOCAL payment, balance, r, month, interest, amort
300   payment = FNPAYMENT(principal, annualRate, years)
510   balance = principal
520   r = annualRate / 100 / 12
525   '
530   GPRINT
540   GPRINT "AMORTIZATION TABLE - FIRST 12 MONTHS"
550   GPRINT "--------------------------------------------------------------------------"
560   GPRINT "Month"; TAB(17); "Payment"; TAB(33); "Interest"; TAB(49); "Principal"; TAB(68); "Balance"
570   GPRINT "--------------------------------------------------------------------------"
575   '
580   FOR month = 1 TO 12
590     IF annualRate = 0 THEN
600       interest = 0
610     ELSE
620       interest = balance * r
630     END IF
635     '
640     amort = payment - interest
650     balance = balance - amort
660     IF balance < 0 THEN balance = 0
665     '
670     GPRINT USING "##"; month;
675     GPRINT TAB(8);
680     GPRINT USING "#,###,###,###.##"; payment;
690     GPRINT TAB(25);
700     GPRINT USING "#,###,###,###.##"; interest;
710     GPRINT TAB(42);
720     GPRINT USING "#,###,###,###.##"; amort;
730     GPRINT TAB(59);
740     GPRINT USING "#,###,###,###.##"; balance
750   NEXT month
755 SUBEND
760 '
770 MODE 800 : SMALLFONT
780 CALL SHOWHEADER
790 GINPUT "Loan principal      : ", principal
800 GINPUT "Annual interest (%) : ", annualRate
810 GINPUT "Term (years)        : ", years
820 '
830 IF principal <= 0 THEN GPRINT "Principal must be greater than zero.":GOTO 790
840 IF annualRate < 0 THEN GPRINT "Interest rate cannot be negative.":GOTO 800
850 IF years <= 0 THEN GPRINT "Years must be greater than zero.":GOTO 810
860 '
870 payment = FNPAYMENT(principal, annualRate, years)
880 months = years * 12
890 totalPaid = payment * months
900 totalIntr = totalPaid - principal
905 '
910 GPRINT
915 GPRINT "===================================="
920 GPRINT "RESULTS"
925 GPRINT "===================================="
930 GPRINT USING "Monthly payment:    ###,###,###.##"; payment
935 GPRINT USING "Total paid:         ###,###,###.##"; totalPaid
940 GPRINT USING "Total interest:     ###,###,###.##"; totalIntr
945 GPRINT
950 '
955 GINPUT "Show first-year table (Y/N)? ", resp$
960 resp$ = TRIM$(UPPER$(resp$))
965 IF resp$ = "Y" THEN CALL YEAR1TABLE(principal, annualRate, years)
970 '
975 GPRINT
980 GPRINT "End of program."
985 END
