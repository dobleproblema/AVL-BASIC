100 DEF SUB SHOWHEADER
110   PRINT "===================================="
120   PRINT "   BASIC LOAN CALCULATOR"
130   PRINT "   Classic + modern features"
140   PRINT "===================================="
150   PRINT
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
530   PRINT
540   PRINT "AMORTIZATION TABLE - FIRST 12 MONTHS"
550   PRINT "--------------------------------------------------------------------------"
560   PRINT "Month"; TAB(17); "Payment"; TAB(33); "Interest"; TAB(49); "Principal"; TAB(68); "Balance"
570   PRINT "--------------------------------------------------------------------------"
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
670     PRINT using "##"; month;
675     PRINT TAB(8);
680     PRINT USING "#,###,###,###.##"; payment;
690     PRINT TAB(25);
700     PRINT USING "#,###,###,###.##"; interest;
710     PRINT TAB(42);
720     PRINT USING "#,###,###,###.##"; amort;
730     PRINT TAB(59);
740     PRINT USING "#,###,###,###.##"; balance
750   NEXT month
755 SUBEND
760 '
770 CLS
780 CALL SHOWHEADER
790 INPUT "Loan principal      : ", principal
800 INPUT "Annual interest (%) : ", annualRate
810 INPUT "Term (years)        : ", years
820 '
830 IF principal <= 0 THEN PRINT "Principal must be greater than zero.": GOTO 790
840 IF annualRate < 0 THEN PRINT "Interest rate cannot be negative.": GOTO 800
850 IF years <= 0 THEN PRINT "Years must be greater than zero.": GOTO 810
860 '
870 payment = FNPAYMENT(principal, annualRate, years)
880 months = years * 12
890 totalPaid = payment * months
900 totalIntr = totalPaid - principal
905 '
910 PRINT
915 PRINT "===================================="
920 PRINT "RESULTS"
925 PRINT "===================================="
930 PRINT USING "Monthly payment:    ###,###,###.##"; payment
935 PRINT USING "Total paid:         ###,###,###.##"; totalPaid
940 PRINT USING "Total interest:     ###,###,###.##"; totalIntr
945 PRINT
950 '
955 INPUT "Show first-year table (Y/N)? ", resp$
960 resp$ = TRIM$(UPPER$(resp$))
965 IF resp$ = "Y" THEN CALL YEAR1TABLE(principal, annualRate, years)
970 '
975 PRINT
980 PRINT "End of program."
985 END
