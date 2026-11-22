(*  FsCommands — the meaning of `FsCommand` (shir_nodes/fs_command.node).

    `rm -f a b`, `mkdir -p d`, `touch f` are the corpus's file-mutating
    externals, and the node exists because each carries a rule that is easy to
    get wrong.  Every rule below was MEASURED on this box (the answers are in
    the comments):

      rm -f missing        0, and nothing changes
      rm -f existing       0, and the path is gone
      rm missing           1 (without -f a missing operand is an error)
      mkdir existing       1 ("File exists")
      mkdir missing/child  1 ("No such file or directory" — the parent rule)
      mkdir -p missing/child  0, and every ancestor is created
      mkdir -p existing    0 (with -p an existing target is fine)
      touch existing       0 (updates the time, never fails)

    The aggregate rule is the node's `$?` contract: a command over several
    operands exits 0 EXACTLY when every operand's step succeeded, so one
    failing operand fails the command even though the others take effect.

    A state is a LIST of existing paths, not a set: `Set` has no code equations,
    so every `by eval` pin over a set would be a proof obligation nobody could
    discharge.  Insertion keeps paths unique (`addp`), so a state is a
    duplicate-free list and comparisons normalise through `sort`.  A path is a
    component list, so "ancestor" is a prefix and "parent" is `butlast`.  The
    root `[]` is always a directory, so every state below starts with it.

    Build:  isabelle build -D sh2perl/docs/fm/FsCommands
*)

theory ShIR_FsCommands
  imports Main
begin

type_synonym path = "nat list"
type_synonym state = "path list"

datatype cmd = Rm bool | Mkdir bool | Touch

section \<open>Paths\<close>

fun is_pref :: "path \<Rightarrow> path \<Rightarrow> bool" where
  "is_pref [] _ = True"
| "is_pref (_ # _) [] = False"
| "is_pref (x # xs) (y # ys) = (x = y \<and> is_pref xs ys)"

fun parent :: "path \<Rightarrow> path" where
  "parent [] = []"
| "parent [_] = []"
| "parent (x # y # ys) = x # parent (y # ys)"

text \<open>An ancestor drops the LAST component (`a/b/c` -> `a/b` -> `a` -> root), not
  the first: `mkdir -p a/b` creates `a`, and the root `[]` is always there.
  (The first version of this theory dropped the first component, which made
  `mkdir -p a/b` "create" `b`; the eval pin below caught it.)\<close>

text \<open>Written through `prefixes` (a structural recursion) and reversed, so the
  definition needs no termination argument about `parent` — the first attempt
  recursed on `parent` and Isabelle could not find a lexicographic order for
  it.\<close>

fun prefixes :: "path \<Rightarrow> path list" where
  "prefixes [] = [[]]"
| "prefixes (x # xs) = [] # map (\<lambda>p. x # p) (prefixes xs)"

definition ancestors :: "path \<Rightarrow> path list" where
  "ancestors p = rev (prefixes p)"

lemma prefixes_include_self: "p \<in> set (prefixes p)"
  by (induction p) auto

lemma prefixes_include_root: "[] \<in> set (prefixes p)"
  by (induction p) auto

lemma ancestors_include_self: "p \<in> set (ancestors p)"
  by (simp add: ancestors_def prefixes_include_self)

lemma ancestors_include_root: "[] \<in> set (ancestors p)"
  by (simp add: ancestors_def prefixes_include_root)

lemma ancestors_length: "length (ancestors p) = length p + 1"
proof (induction p)
  case Nil then show ?case by (simp add: ancestors_def)
next
  case (Cons x xs)
  then show ?case by (simp add: ancestors_def)
qed

section \<open>State updates\<close>

text \<open>`addp` keeps a path list duplicate-free; `rmp` removes a path (it is
  `-f`, so removing a missing path is a no-op); `mkdir_p` adds every
  ancestor.\<close>

fun addp :: "path \<Rightarrow> state \<Rightarrow> state" where
  "addp p s = (if p \<in> set s then s else p # s)"

definition rmp :: "path \<Rightarrow> state \<Rightarrow> state" where
  "rmp p s = filter (\<lambda>q. q \<noteq> p) s"

definition mkdir_p :: "state \<Rightarrow> path \<Rightarrow> state" where
  "mkdir_p s p = foldr addp s (ancestors p)"

section \<open>One operand\<close>

text \<open>`step` is the documented per-operand rule, returning the new state and
  the operand's status.  `Rm` carries `-f`; `Mkdir` carries `-p`; `Touch` has
  no flag that changes its answer.\<close>

fun step :: "cmd \<Rightarrow> state \<Rightarrow> path \<Rightarrow> state \<times> nat" where
  "step (Rm force) s p = (rmp p s, (if force \<or> p \<in> set s then 0 else 1))"
| "step (Mkdir True) s p = (mkdir_p s p, 0)"
| "step (Mkdir False) s p =
     (if p \<in> set s \<or> parent p \<notin> set s then (s, 1) else (addp p s, 0))"
| "step Touch s p = (addp p s, 0)"

text \<open>The trace of a multi-operand command, one entry per operand.\<close>

fun steps :: "cmd \<Rightarrow> state \<Rightarrow> path list \<Rightarrow> (state \<times> nat) list" where
  "steps c s [] = []"
| "steps c s (p # ps) =
     (case step c s p of (s1, r) \<Rightarrow> (s1, r) # steps c s1 ps)"

definition final_state :: "cmd \<Rightarrow> state \<Rightarrow> path list \<Rightarrow> state" where
  "final_state c s ps = (case rev (steps c s ps) of [] \<Rightarrow> s | (s1, _) # _ \<Rightarrow> s1)"

definition run :: "cmd \<Rightarrow> state \<Rightarrow> path list \<Rightarrow> state \<times> nat" where
  "run c s ps =
     (final_state c s ps,
      (if list_all (\<lambda>e. snd e = 0) (steps c s ps) then 0 else 1))"

section \<open>The aggregate status\<close>

text \<open>`command_sound` — the obligation `fs_command.node` cites.  A command's
  exit status is 0 exactly when every operand's step succeeded: the renderer
  may fold the loop in any order, and a single failing operand makes the whole
  command exit 1 even though the other operands took effect.\<close>

theorem command_sound:
  "snd (run c s ps) = 0 \<longleftrightarrow> list_all (\<lambda>e. snd e = 0) (steps c s ps)"
  by (simp add: run_def)

corollary one_failing_operand_fails_the_command:
  assumes "snd (step c s p) \<noteq> 0"
  shows "snd (run c s [p]) \<noteq> 0"
  using assms by (simp add: run_def split: prod.splits)

lemma empty_operand_list_succeeds: "snd (run c s []) = 0"
  by (simp add: run_def)

lemma trace_is_one_per_operand: "length (steps c s ps) = length ps"
  by (induction ps arbitrary: s) (auto split: prod.splits)

section \<open>The measured rules, pinned\<close>

text \<open>A state's list order is INSERTION order (`addp` prepends; `rmp` filters),
  so the equalities below spell it out.  `sort` is not usable here: it needs a
  `linorder` instance for `path = nat list`, which HOL has no default for.
  (Measured answers are in the header.)\<close>

text \<open>`rm -f` on a missing path: status 0, nothing changes.\<close>

lemma rm_f_missing_ok: "run (Rm True) [[]] [[2]] = ([[]], 0)"
  by eval

lemma rm_f_removes_existing: "run (Rm True) [[], [1], [2]] [[1]] = ([[], [2]], 0)"
  by eval

lemma rm_missing_without_force_fails: "snd (run (Rm False) [[]] [[2]]) = 1"
  by eval

lemma rm_existing_without_force_ok: "snd (run (Rm False) [[], [1]] [[1]]) = 0"
  by eval

text \<open>`mkdir -p a/b` creates `a` as well as `a/b`, and `-p` on an existing
  target is status 0.\<close>

lemma mkdir_p_creates_ancestors:
  "fst (run (Mkdir True) [[]] [[2, 3]]) = [[2, 3], [2], []]"
  by eval

lemma mkdir_p_existing_ok: "snd (run (Mkdir True) [[], [2]] [[2]]) = 0"
  by eval

text \<open>Plain `mkdir` fails on an existing target, fails on a missing parent
  (`mkdir: No such file or directory`), and succeeds when the parent exists.\<close>

lemma mkdir_existing_fails: "snd (run (Mkdir False) [[], [2]] [[2]]) = 1"
  by eval

lemma mkdir_missing_parent_fails: "snd (run (Mkdir False) [[]] [[2, 3]]) = 1"
  by eval

lemma mkdir_plain_creates: "fst (run (Mkdir False) [[]] [[3]]) = [[3], []]"
  by eval

text \<open>`touch` creates a missing path and is idempotent on an existing one.\<close>

lemma touch_creates: "run Touch [[]] [[1]] = ([[1], []], 0)"
  by eval

lemma touch_idempotent: "run Touch [[], [1]] [[1]] = ([[], [1]], 0)"
  by eval

text \<open>One failing operand fails the whole command while the others still take
  effect: `mkdir -p a b` where `a` exists is 0; the same without `-p` is 1 and
  `b` is created anyway (measured).\<close>

lemma mkdir_p_one_existing_one_new:
  "snd (run (Mkdir True) [[], [1]] [[1], [2]]) = 0"
  by eval

lemma mkdir_one_existing_one_new:
  "run (Mkdir False) [[], [1]] [[1], [2]] = ([[2], [], [1]], 1)"
  by eval

end
