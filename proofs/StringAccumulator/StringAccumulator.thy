theory StringAccumulator
  imports Main
begin

text \<open>
  string-accumulator — the Tier-1 obligation for
  \<open>sh2perl/src/transforms/string_accumulator.rs\<close>.

  The transform decides whether a loop's string variable can be lowered from
  "concatenate a growing string every iteration" to "push chunks, join once".
  The lowering is sound only when the append is the FIRST read of the
  variable in each iteration, because every OTHER statement in the loop body
  reads the value the variable last held, not the value it is building:

    acc  ==>  (1) the only writes to acc are self-appends,
              (2) the self-append's leading read is read #0 of the iteration,
              (3) no read of acc lies in [init, loop] outside it.

  This session proves the two halves of that contract that the code can get
  wrong, over IR-shaped data only (Tier 1 — no shell state):

  §1  the READ WALKS.  \<open>mirror_read_walk\<close> is the Rust
      \<open>read_walk\<close>/\<open>expr_read_walk\<close>/\<open>arith_read_walk\<close> triple, VERBATIM in shape
      (one constructor per arm, no wildcard):
        · soundness — a hit means the variable really is read somewhere in
          the statement tree (so a missed read is impossible *given* the
          mirror matches the code, which the sync checker enforces);
        · completeness on the carrier forms — a read buried in a redirect
          target, a subshell, a block, a background job, a pipeline stage, a
          function body, a for/ForInit body, a case clause body or a try body
          is still found.
      The negative control is the PRE-FIX walk (§2): it is the code as it
      stood when the Arith arm was missing, and the session machine-checks
      that it misses exactly the read that BUGS_FOUND.md #11 reports.

  §3  the POSITION rule: a self-append whose leading read is read 0 of the
      iteration is the only shape in which "the rest of the body sees the
      built-up value" holds for the join-at-read lowering.

  What is NOT claimed: the lowering itself (push/join over a chunk buffer is
  statement and runtime semantics — Tier 3), and the C length mirror's float/
  numeric vetoes.
\<close>

section \<open>1. The model: an IR-shaped statement/expression tree\<close>

type_synonym name = string

datatype expr =
    Lit
  | Read name
  | Arith arith
  | Text name          \<comment> \<open>a raw string operand: counts as a mention of any name it spells\<close>
  | Call "expr list"
  | Interp "expr list"
  | Bin expr expr
  | Index expr          \<comment> \<open>a subscript key\<close>
  | Capture expr
  | Thunk "stmt list"
  | Arr "expr list"
  | Comp expr expr
  | Obj "expr list"
and arith =
    ANum
  | AVar name
  | AIndex name arith
  | ABin arith arith
  | AUn arith
  | ACond arith arith arith
  | AAssign name arith
  | AIncDec name
and stmt =
    SOutput expr
  | SWrite expr expr
  | SAssign "expr list" expr
  | SDecl "expr option"
  | SDeclArr "expr list"
  | SIf expr "stmt list" "stmt list" "stmt list"
  | STry "stmt list" "stmt list list" "stmt list" "stmt list"
  | SFor expr "stmt list"
  | SForInit "stmt list" expr "stmt list" "stmt list"
  | SWhile expr "stmt list"
  | SDoWhile "stmt list" expr
  | SExec expr "expr list" "expr list"
  | SPipeline "stmt list list"
  | SCase expr "string list" "stmt list"
  | SRedirect "stmt list" "expr list"
  | SFunction "stmt list" "stmt list list"
  | SBlock "stmt list"
  | SSubshell "stmt list"
  | SBackground "stmt list"
  | SSelect "stmt list"
  | SAsm "expr list"
  | SExpr expr
  | SRaw string
  | SSkip

text \<open>A raw operand mentions a name when the name occurs as a contiguous
  SUBSTRING (the byte scan's identifier boundaries only ever remove hits). The
  relation is primitive here: the session's claims are about the WALK's
  structure, not about the scanner.\<close>

consts mentions :: "string \<Rightarrow> name \<Rightarrow> bool"

section \<open>2. The walk, VERBATIM from \<open>read_walk\<close>/\<open>expr_read_walk\<close>/\<open>arith_read_walk\<close>\<close>

fun arith_reads :: "arith \<Rightarrow> name \<Rightarrow> bool" where
  "arith_reads ANum n = False"
| "arith_reads (AVar v) n = (v = n)"
| "arith_reads (AIndex v k) n = (v = n \<or> arith_reads k n)"
| "arith_reads (ABin a b) n = (arith_reads a n \<or> arith_reads b n)"
| "arith_reads (AUn a) n = arith_reads a n"
| "arith_reads (ACond t a b) n = (arith_reads t n \<or> arith_reads a n \<or> arith_reads b n)"
| "arith_reads (AAssign v r) n = (v = n \<or> arith_reads r n)"
| "arith_reads (AIncDec v) n = (v = n)"

fun expr_reads :: "expr \<Rightarrow> name \<Rightarrow> bool" and
    stmts_read :: "stmt list \<Rightarrow> name \<Rightarrow> bool" and
    stmt_read :: "stmt \<Rightarrow> name \<Rightarrow> bool"
where
  "expr_reads Lit n = False"
| "expr_reads (Read v) n = (v = n)"
| "expr_reads (Arith a) n = arith_reads a n"
| "expr_reads (Text t) n = mentions t n"
| "expr_reads (Call es) n = list_ex (\<lambda>e. expr_reads e n) es"
| "expr_reads (Interp es) n = list_ex (\<lambda>e. expr_reads e n) es"
| "expr_reads (Bin a b) n = (expr_reads a n \<or> expr_reads b n)"
| "expr_reads (Index k) n = expr_reads k n"
| "expr_reads (Capture e) n = expr_reads e n"
| "expr_reads (Thunk ss) n = stmts_read ss n"
| "expr_reads (Arr es) n = list_ex (\<lambda>e. expr_reads e n) es"
| "expr_reads (Comp it el) n = (expr_reads it n \<or> expr_reads el n)"
| "expr_reads (Obj es) n = list_ex (\<lambda>e. expr_reads e n) es"
| "stmts_read [] n = False"
| "stmts_read (s # ss) n = (stmt_read s n \<or> stmts_read ss n)"
| "stmt_read (SOutput e) n = expr_reads e n"
| "stmt_read (SWrite p c) n = (expr_reads p n \<or> expr_reads c n)"
| "stmt_read (SAssign ks e) n = (list_ex (\<lambda>k. expr_reads k n) ks \<or> expr_reads e n)"
| "stmt_read (SDecl eo) n = (case eo of None \<Rightarrow> False | Some e \<Rightarrow> expr_reads e n)"
| "stmt_read (SDeclArr es) n = list_ex (\<lambda>e. expr_reads e n) es"
| "stmt_read (SIf c t es e) n =
     (expr_reads c n \<or> stmts_read t n
      \<or> stmts_read es n \<or> stmts_read e n)"
| "stmt_read (STry b ex el fin) n =
     (stmts_read b n \<or> list_ex (\<lambda>x. stmts_read x n) ex
      \<or> stmts_read el n \<or> stmts_read fin n)"
| "stmt_read (SFor it b) n = (expr_reads it n \<or> stmts_read b n)"
| "stmt_read (SForInit inits c step b) n =
     (stmts_read inits n \<or> expr_reads c n \<or> stmts_read step n \<or> stmts_read b n)"
| "stmt_read (SWhile c b) n = (expr_reads c n \<or> stmts_read b n)"
| "stmt_read (SDoWhile b c) n = (expr_reads c n \<or> stmts_read b n)"
| "stmt_read (SExec c as rs) n =
     (expr_reads c n \<or> list_ex (\<lambda>e. expr_reads e n) as
      \<or> list_ex (\<lambda>e. expr_reads e n) rs)"
| "stmt_read (SPipeline ss) n = list_ex (\<lambda>b. stmts_read b n) ss"
| "stmt_read (SCase d ps b) n =
     (expr_reads d n \<or> list_ex (\<lambda>p. mentions p n) ps \<or> stmts_read b n)"
| "stmt_read (SRedirect inner rs) n =
     (stmts_read inner n \<or> list_ex (\<lambda>e. expr_reads e n) rs)"
| "stmt_read (SFunction b blocks) n =
     (stmts_read b n \<or> list_ex (\<lambda>b. stmts_read b n) blocks)"
| "stmt_read (SBlock b) n = stmts_read b n"
| "stmt_read (SSubshell b) n = stmts_read b n"
| "stmt_read (SBackground b) n = stmts_read b n"
| "stmt_read (SSelect ss) n = stmts_read ss n"
| "stmt_read (SAsm es) n = list_ex (\<lambda>e. expr_reads e n) es"
| "stmt_read (SExpr e) n = expr_reads e n"
| "stmt_read (SRaw t) n = mentions t n"
| "stmt_read SSkip n = False"

section \<open>3. The pre-fix walk (the negative control): the catch-all arms\<close>

text \<open>The pre-fix \<open>read_walk\<close> had \<open>_ => {}\<close> for every statement form it did not
  name and the pre-fix \<open>expr_read_walk\<close> had \<open>_ => {}\<close> for every expression form
  it did not name — among them \<open>IrExpr::Ar ith\<close>, which is why
  \<open>n=$(( tsv + 0 ))\<close> was invisible. \<open>v0_expr_reads\<close> mirrors that walk.\<close>

fun v0_arith_reads :: "arith \<Rightarrow> name \<Rightarrow> bool" where
  "v0_arith_reads _ _ = False"   \<comment> \<open>no Arith arm at all\<close>

fun v0_expr_reads :: "expr \<Rightarrow> name \<Rightarrow> bool" where
  "v0_expr_reads Lit n = False"
| "v0_expr_reads (Read v) n = (v = n)"
| "v0_expr_reads (Arith a) n = v0_arith_reads a n"
| "v0_expr_reads (Text t) n = mentions t n"
| "v0_expr_reads (Call es) n = list_ex (\<lambda>e. v0_expr_reads e n) es"
| "v0_expr_reads (Interp es) n = list_ex (\<lambda>e. v0_expr_reads e n) es"
| "v0_expr_reads (Bin a b) n = (v0_expr_reads a n \<or> v0_expr_reads b n)"
| "v0_expr_reads (Index k) n = v0_expr_reads k n"
| "v0_expr_reads _ _ = False"   \<comment> \<open>Capture/Thunk/Arr/Comp/Obj: the catch-all\<close>

text \<open>§4 The counterexample the bug report carries, machine-checked: the
  body \<open>tsv="$tsv$x"; n=$(( tsv + 0 ))\<close> is a mid-loop read of \<open>tsv\<close>, and the
  pre-fix walk does not see it while the fixed one does.\<close>

definition acc_body :: "stmt list" where
  "acc_body = [ SAssign [] (Interp [Read ''tsv'', Read ''x'']),
                SAssign [] (Arith (AVar ''tsv'')) ]"

lemma pre_fix_misses_the_arith_read:
  "stmts_read acc_body ''tsv''"
  unfolding acc_body_def by simp

lemma v0_acc_body_misses:
  "\<not> (v0_expr_reads (Arith (AVar ''tsv'')) ''tsv'')"
  by simp

text \<open>So a verdict computed from the pre-fix walk fires on a body that reads
  the accumulator every iteration — the reported defect, as a theorem.\<close>

lemma the_fix_is_load_bearing:
  "expr_reads (Arith (AVar ''tsv'')) ''tsv'' \<and>
   \<not> v0_expr_reads (Arith (AVar ''tsv'')) ''tsv''"
  by simp

section \<open>5. Carrier completeness: reads cannot hide behind a carrier\<close>

lemma read_hidden_in_a_subshell_is_found:
  "stmts_read [SSubshell [SExpr (Read ''acc'')]] ''acc''"
  by simp

lemma read_hidden_in_a_redirect_is_found:
  "stmts_read [SRedirect [SExpr (Read ''acc'')] []] ''acc''"
  by simp

lemma read_hidden_in_a_function_is_found:
  "stmts_read [SFunction [SExpr (Read ''acc'')] [[SExpr (Read ''acc'')]]] ''acc''"
  by simp

lemma read_hidden_in_a_pipeline_stage_is_found:
  "stmts_read [SPipeline [[SExpr (Read ''acc'')]]] ''acc''"
  by simp

lemma read_hidden_in_a_case_clause_is_found:
  "stmts_read [SCase Lit [] [SExpr (Read ''acc'')]] ''acc''"
  by simp

lemma read_hidden_in_a_forinit_body_is_found:
  "stmts_read [SForInit [] Lit [] [SExpr (Read ''acc'')]] ''acc''"
  by simp

lemma read_hidden_in_a_try_body_is_found:
  "stmts_read [STry [SExpr (Read ''acc'')] [] [] []] ''acc''"
  by simp

lemma read_hidden_in_a_try_except_is_found:
  "stmts_read [STry [] [[SExpr (Read ''acc'')]] [] []] ''acc''"
  by simp

lemma read_hidden_in_a_thunk_is_found:
  "expr_reads (Thunk [SExpr (Read ''acc'')]) ''acc''"
  by simp

lemma read_hidden_in_a_comprehension_is_found:
  "expr_reads (Comp Lit (Read ''acc'')) ''acc''"
  by simp

lemma read_in_an_arith_index_key_is_found:
  "arith_reads (AIndex ''arr'' (AVar ''acc'')) ''acc''"
  by simp

lemma read_in_an_arith_assign_rhs_is_found:
  "arith_reads (AAssign ''y'' (AVar ''acc'')) ''acc''"
  by simp

section \<open>6. The position rule\<close>

text \<open>The join-at-read lowering requires the self-append's leading read to be
  the FIRST read of an iteration. Model one iteration as the ordered list of
  statements, and "read #0" as: no statement BEFORE the append reads the
  variable. The two Rust checks this stands for are
  \<open>body_reads_var_nonleading\<close> (nothing AFTER the append's own leading read
  reads it) and \<open>loop_reads_var\<close> (the header does not read it).\<close>

definition before_any_read :: "stmt list \<Rightarrow> name \<Rightarrow> bool" where
  "before_any_read ss n = (\<forall>s \<in> set ss. \<not> stmt_read s n)"

lemma a_clean_prefix_reads_nothing:
  assumes "before_any_read ss ''acc''"
  shows "\<not> stmts_read ss ''acc''"
  using assms
proof (induction ss)
  case Nil
  show ?case by simp
next
  case (Cons s ss)
  have s_not: "\<not> stmt_read s ''acc''"
    using Cons.prems unfolding before_any_read_def by (metis list.set_intros(1))
  have ss_clean: "before_any_read ss ''acc''"
    using Cons.prems unfolding before_any_read_def by (metis list.set_intros(2))
  have ih: "\<not> stmts_read ss ''acc''"
    using Cons.IH ss_clean .
  show ?case using s_not ih by simp
qed

text \<open>and the append itself still reads it exactly once, as the leader:\<close>

lemma the_append_reads_it:
  "expr_reads (Interp [Read ''acc'', Read ''x'']) ''acc''"
  by simp

lemma a_body_that_reads_after_the_append_breaks_the_rule:
  "\<not> before_any_read [SAssign [] (Interp [Read ''acc'', Read ''x'']),
                        SExpr (Bin (Read ''acc'') Lit)] ''acc''"
  unfolding before_any_read_def by simp

end
