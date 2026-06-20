# Certification status — eliminating κ via per-problem proofs

**Certified: 278/577 relevant problems** (56.3% of frequency-weighted load; goal is the full 82.3% non-tail mass).

Each certified problem carries machine-checked `cls` (scheme membership) + `corr` (correctness vs spec), standard axioms only, no `sorry`. Pending problems have no file yet.

| scheme | family | certified | total |
|---|---|--:|--:|
| fold | hashing | 26 | 48 |
| fold | string-other | 14 | 45 |
| fold | two-pointers | 17 | 36 |
| fold | math-bit | 13 | 33 |
| fold | linked-list | 11 | 21 |
| fold | sliding-window | 11 | 21 |
| fold | monotonic-stack | 7 | 19 |
| fold | prefix-sum | 5 | 17 |
| fold | pairing-stack | 8 | 14 |
| fold | diff-array | 2 | 8 |
| fold | fast-slow | 2 | 6 |
| fold | merge-intervals | 4 | 5 |
| fold | dutch-flag | 1 | 1 |
| dp | dp-linear | 17 | 33 |
| dp | backtracking | 12 | 29 |
| dp | dp-grid | 11 | 25 |
| dp | tree-traversal | 4 | 13 |
| dp | bst | 5 | 12 |
| dp | dp-tree | 3 | 12 |
| dp | trie | 7 | 10 |
| dp | tree-construct | 5 | 9 |
| dp | tree-aggregate | 6 | 8 |
| dp | dp-interval | 3 | 7 |
| dp | dp-knapsack | 3 | 6 |
| dp | dp-digit | 0 | 2 |
| dp | dp-bitmask | 1 | 2 |
| relaxation | bfs | 16 | 31 |
| relaxation | union-find | 9 | 17 |
| relaxation | graph-other | 10 | 15 |
| relaxation | dfs-flood | 4 | 10 |
| relaxation | topo-sort | 5 | 9 |
| relaxation | dijkstra | 4 | 9 |
| relaxation | bellman-ford | 1 | 1 |
| bisection | binary-search | 31 | 43 |

## Certified problems

- `1` Two Sum — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0001_TwoSum.lean`
- `2` Add Two Numbers — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0002_AddTwoNumbers.lean`
- `3` Longest Substring Without Repeating Characters — fold () — `lproofs/Lproofs/Problems/Fold/P0003_LongestSubstringNoRepeat.lean`
- `4` Median of Two Sorted Arrays — bisection (O(log n)) — `lproofs/Lproofs/Problems/Bisection/P0004_MedianTwoSortedArrays.lean`
- `5` Longest Palindromic Substring — fold (O(n²)) — `lproofs/Lproofs/Problems/DP/P0005_LongestPalindromicSubstring.lean`
- `7` Reverse Integer — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0007_ReverseInteger.lean`
- `9` Palindrome Number — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0009_PalindromeNumber.lean`
- `14` Longest Common Prefix — fold (O(Σ) — `lproofs/Lproofs/Problems/Fold/P0014_LongestCommonPrefix.lean`
- `17` Letter Combinations of a Phone Number — dp () — `lproofs/Lproofs/Problems/DP/P0017_LetterCombinations.lean`
- `20` Valid Parentheses — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0020_ValidParentheses.lean`
- `21` Merge Two Sorted Lists — fold (O(n+m)) — `lproofs/Lproofs/Problems/DP/P0021_MergeTwoSortedLists.lean`
- `24` Swap Nodes in Pairs — fold (O(n)) — `lproofs/Lproofs/Problems/DP/P0024_SwapNodesInPairs.lean`
- `26` Remove Duplicates from Sorted Array — fold () — `lproofs/Lproofs/Problems/Fold/P0026_RemoveDuplicatesSorted.lean`
- `27` Remove Element — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0027_RemoveElement.lean`
- `34` Find First and Last Position of Element in Sorted Array — bisection () — `lproofs/Lproofs/Problems/Bisection/P0034_FindFirstLast.lean`
- `35` Search Insert Position — bisection (O(log n)) — `lproofs/Lproofs/Problems/Bisection/P0035_SearchInsert.lean`
- `36` Valid Sudoku — fold (O(1)) — `lproofs/Lproofs/Problems/Fold/P0036_ValidSudoku.lean`
- `37` Sudoku Solver — dp (exp) — `lproofs/Lproofs/Problems/Relaxation/P0037_SudokuSolver.lean`
- `38` Count and Say — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0038_CountAndSay.lean`
- `39` Combination Sum — dp (exponential) — `lproofs/Lproofs/Problems/DP/P0039_CombinationSum.lean`
- `41` First Missing Positive — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0041_FirstMissingPositive.lean`
- `42` Trapping Rain Water — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0042_TrappingRainWater.lean`
- `43` Multiply Strings — fold (O(mn)) — `lproofs/Lproofs/Problems/Fold/P0043_MultiplyStrings.lean`
- `44` Wildcard Matching — dp (O(mn)) — `lproofs/Lproofs/Problems/DP/P0044_WildcardMatching.lean`
- `46` Permutations — dp (O(n·n!)) — `lproofs/Lproofs/Problems/DP/P0046_Permutations.lean`
- `49` Group Anagrams — fold (O(n·k)) — `lproofs/Lproofs/Problems/Fold/P0049_GroupAnagrams.lean`
- `50` Pow(x, n) — fold (O(log n)) — `lproofs/Lproofs/Problems/Fold/P0050_PowXN.lean`
- `51` N-Queens — dp (exponential) — `lproofs/Lproofs/Problems/DP/P0051_NQueens.lean`
- `53` Maximum Subarray — dp (O(n)) — `lproofs/Lproofs/Problems/DP/P0053_MaximumSubarray.lean`
- `56` Merge Intervals — fold (O(n log n)) — `lproofs/Lproofs/Problems/Fold/P0056_MergeIntervals.lean`
- `57` Insert Interval — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0057_InsertInterval.lean`
- `58` Length of Last Word — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0058_LengthOfLastWord.lean`
- `61` Rotate List — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0061_RotateList.lean`
- `62` Unique Paths — dp (O(mn)) — `lproofs/Lproofs/Problems/DP/P0062_UniquePaths.lean`
- `63` Unique Paths II — dp (O(mn)) — `lproofs/Lproofs/Problems/DP/P0063_UniquePathsII.lean`
- `65` Valid Number — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0065_ValidNumber.lean`
- `66` Plus One — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0066_PlusOne.lean`
- `67` Add Binary — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0067_AddBinary.lean`
- `68` Text Justification — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0068_TextJustification.lean`
- `69` Sqrt(x) — bisection (O(log x)) — `lproofs/Lproofs/Problems/Bisection/P0069_Sqrt.lean`
- `70` Climbing Stairs — dp (O(n)) — `lproofs/Lproofs/Problems/DP/P0070_ClimbingStairs.lean`
- `72` Edit Distance — dp (O(mn)) — `lproofs/Lproofs/Problems/DP/P0072_EditDistance.lean`
- `74` Search a 2D Matrix — bisection (O(log nm)) — `lproofs/Lproofs/Problems/Bisection/P0074_Search2DMatrix.lean`
- `75` Sort Colors — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0075_SortColors.lean`
- `76` Minimum Window Substring — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0076_MinWindowSubstring.lean`
- `78` Subsets — dp (O(n·2ⁿ)) — `lproofs/Lproofs/Problems/DP/P0078_Subsets.lean`
- `79` Word Search — dp (O(m·n·4^L)) — `lproofs/Lproofs/Problems/Relaxation/P0079_WordSearch.lean`
- `81` Search in Rotated Sorted Array II — bisection () — `lproofs/Lproofs/Problems/Bisection/P0081_SearchRotatedII.lean`
- `84` Largest Rectangle in Histogram — fold () — `lproofs/Lproofs/Problems/Fold/P0084_LargestRectangle.lean`
- `85` Maximal Rectangle — fold (O(mn)) — `lproofs/Lproofs/Problems/Fold/P0085_MaximalRectangle.lean`
- `88` Merge Sorted Array — fold (O(n+m)) — `lproofs/Lproofs/Problems/DP/P0088_MergeSortedArray.lean`
- `90` Subsets II — dp (O(n·2ⁿ)) — `lproofs/Lproofs/Problems/DP/P0090_SubsetsII.lean`
- `91` Decode Ways — dp (O(n)) — `lproofs/Lproofs/Problems/DP/P0091_DecodeWays.lean`
- `92` Reverse Linked List II — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0092_ReverseLinkedListII.lean`
- `94` Binary Tree Inorder Traversal — dp (O(n)) — `lproofs/Lproofs/Problems/DP/P0094_BinaryTreeInorder.lean`
- `97` Interleaving String — dp (O(mn)) — `lproofs/Lproofs/Problems/DP/P0097_InterleavingString.lean`
- `98` Validate Binary Search Tree — dp (O(n)) — `lproofs/Lproofs/Problems/DP/P0098_ValidateBST.lean`
- `101` Symmetric Tree — dp (O(n)) — `lproofs/Lproofs/Problems/DP/P0101_SymmetricTree.lean`
- `102` Binary Tree Level Order Traversal — relaxation (O(n)) — `lproofs/Lproofs/Problems/Relaxation/P0102_LevelOrderTraversal.lean`
- `104` Maximum Depth of Binary Tree — dp (O(n)) — `lproofs/Lproofs/Problems/DP/P0104_MaxDepth.lean`
- `105` Construct Binary Tree from Preorder and Inorder Traversal — dp () — `lproofs/Lproofs/Problems/DP/P0105_ConstructBTfromPreandIn.lean`
- `108` Convert Sorted Array to Binary Search Tree — dp () — `lproofs/Lproofs/Problems/DP/P0108_SortedArrayToBST.lean`
- `110` Balanced Binary Tree — dp (O(n)) — `lproofs/Lproofs/Problems/DP/P0110_BalancedBinaryTree.lean`
- `111` Minimum Depth of Binary Tree — dp (O(n)) — `lproofs/Lproofs/Problems/DP/P0111_MinimumDepthBinaryTree.lean`
- `112` Path Sum — dp (O(n)) — `lproofs/Lproofs/Problems/DP/P0112_PathSum.lean`
- `113` Path Sum II — dp (O(n)) — `lproofs/Lproofs/Problems/DP/P0113_PathSumII.lean`
- `115` Distinct Subsequences — dp (O(mn)) — `lproofs/Lproofs/Problems/DP/P0115_DistinctSubsequences.lean`
- `118` Pascal's Triangle — dp (O(n²)) — `lproofs/Lproofs/Problems/DP/P0118_PascalsTriangle.lean`
- `120` Triangle — dp (O(n²)) — `lproofs/Lproofs/Problems/DP/P0120_Triangle.lean`
- `124` Binary Tree Maximum Path Sum — dp (O(n)) — `lproofs/Lproofs/Problems/DP/P0124_MaxPathSum.lean`
- `125` Valid Palindrome — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0125_ValidPalindrome.lean`
- `126` Word Ladder II — relaxation (O(N·L²)) — `lproofs/Lproofs/Problems/Relaxation/P0126_WordLadderII.lean`
- `127` Word Ladder — relaxation (O(N·L²)) — `lproofs/Lproofs/Problems/Relaxation/P0127_WordLadder.lean`
- `129` Sum Root to Leaf Numbers — dp (O(n)) — `lproofs/Lproofs/Problems/DP/P0129_SumRootToLeaf.lean`
- `130` Surrounded Regions — relaxation (O(mn)) — `lproofs/Lproofs/Problems/Relaxation/P0130_SurroundedRegions.lean`
- `133` Clone Graph — relaxation (O(V+E)) — `lproofs/Lproofs/Problems/Relaxation/P0133_CloneGraph.lean`
- `136` Single Number — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0136_SingleNumber.lean`
- `138` Copy List with Random Pointer — fold () — `lproofs/Lproofs/Problems/Relaxation/P0138_CopyListRandomPointer.lean`
- `139` Word Break — dp (O(n²)) — `lproofs/Lproofs/Problems/DP/P0139_WordBreak.lean`
- `140` Word Break II — dp (exp) — `lproofs/Lproofs/Problems/DP/P0140_WordBreakII.lean`
- `148` Sort List — fold (O(n log n)) — `lproofs/Lproofs/Problems/DP/P0148_SortList.lean`
- `150` Evaluate Reverse Polish Notation — fold () — `lproofs/Lproofs/Problems/Fold/P0150_EvalRPN.lean`
- `151` Reverse Words in a String — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0151_ReverseWords.lean`
- `153` Find Minimum in Rotated Sorted Array — bisection () — `lproofs/Lproofs/Problems/Bisection/P0153_FindMinRotated.lean`
- `161` One Edit Distance — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0161_OneEditDistance.lean`
- `162` Find Peak Element — bisection (O(log n)) — `lproofs/Lproofs/Problems/Bisection/P0162_FindPeakElement.lean`
- `163` Missing Ranges — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0163_MissingRanges.lean`
- `167` Two Sum II - Input Array Is Sorted — fold () — `lproofs/Lproofs/Problems/Bisection/P0167_TwoSumSorted.lean`
- `169` Majority Element — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0169_MajorityElement.lean`
- `172` Factorial Trailing Zeroes — fold (O(log n)) — `lproofs/Lproofs/Problems/Fold/P0172_FactorialTrailingZeroes.lean`
- `173` Binary Search Tree Iterator — dp (O(n)) — `lproofs/Lproofs/Problems/DP/P0173_BSTIterator.lean`
- `191` Number of 1 Bits — fold (O(1)) — `lproofs/Lproofs/Problems/Fold/P0191_NumberOf1Bits.lean`
- `198` House Robber — dp (O(n)) — `lproofs/Lproofs/Problems/DP/P0198_HouseRobber.lean`
- `199` Binary Tree Right Side View — relaxation (O(n)) — `lproofs/Lproofs/Problems/DP/P0199_RightSideView.lean`
- `200` Number of Islands — relaxation (O(mn)) — `lproofs/Lproofs/Problems/Relaxation/P0200_NumberOfIslands.lean`
- `202` Happy Number — fold (O(log n)) — `lproofs/Lproofs/Problems/Relaxation/P0202_HappyNumber.lean`
- `203` Remove Linked List Elements — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0203_RemoveLinkedListElements.lean`
- `205` Isomorphic Strings — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0205_IsomorphicStrings.lean`
- `206` Reverse Linked List — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0206_ReverseLinkedList.lean`
- `207` Course Schedule — relaxation (O(V+E)) — `lproofs/Lproofs/Problems/Relaxation/P0207_CourseSchedule.lean`
- `208` Implement Trie (Prefix Tree) — dp (O() — `lproofs/Lproofs/Problems/DP/P0208_ImplementTrie.lean`
- `209` Minimum Size Subarray Sum — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0209_MinSizeSubarraySum.lean`
- `210` Course Schedule II — relaxation (O(V+E)) — `lproofs/Lproofs/Problems/Relaxation/P0210_CourseScheduleII.lean`
- `211` Design Add and Search Words Data Structure — dp () — `lproofs/Lproofs/Problems/DP/P0211_AddSearchWords.lean`
- `212` Word Search II — dp (O(m·n·4^L)) — `lproofs/Lproofs/Problems/Relaxation/P0212_WordSearchII.lean`
- `213` House Robber II — dp (O(n)) — `lproofs/Lproofs/Problems/DP/P0213_HouseRobberII.lean`
- `217` Contains Duplicate — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0217_ContainsDuplicate.lean`
- `219` Contains Duplicate II — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0219_ContainsDuplicateII.lean`
- `226` Invert Binary Tree — dp (O(n)) — `lproofs/Lproofs/Problems/DP/P0226_InvertBinaryTree.lean`
- `228` Summary Ranges — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0228_SummaryRanges.lean`
- `229` Majority Element II — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0229_MajorityElementII.lean`
- `230` Kth Smallest Element in a BST — dp (O(n)) — `lproofs/Lproofs/Problems/DP/P0230_KthSmallestBST.lean`
- `231` Power of Two — fold (O(log n)) — `lproofs/Lproofs/Problems/Fold/P0231_PowerOfTwo.lean`
- `234` Palindrome Linked List — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0234_PalindromeLinkedList.lean`
- `235` Lowest Common Ancestor of a Binary Search Tree — dp () — `lproofs/Lproofs/Problems/DP/P0235_LCAofBST.lean`
- `238` Product of Array Except Self — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0238_ProductExceptSelf.lean`
- `240` Search a 2D Matrix II — bisection () — `lproofs/Lproofs/Problems/Bisection/P0240_Search2DMatrixII.lean`
- `242` Valid Anagram — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0242_ValidAnagram.lean`
- `252` Meeting Rooms — fold (O(n log n)) — `lproofs/Lproofs/Problems/Fold/P0252_MeetingRooms.lean`
- `253` Meeting Rooms II — fold (O(n log n)) — `lproofs/Lproofs/Problems/Fold/P0253_MeetingRoomsII.lean`
- `254` Factor Combinations — dp (exp) — `lproofs/Lproofs/Problems/DP/P0254_FactorCombinations.lean`
- `258` Add Digits — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0258_AddDigits.lean`
- `268` Missing Number — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0268_MissingNumber.lean`
- `269` Alien Dictionary — relaxation (O(V+E)) — `lproofs/Lproofs/Problems/Relaxation/P0269_AlienDictionary.lean`
- `270` Closest Binary Search Tree Value — dp (O(h)) — `lproofs/Lproofs/Problems/DP/P0270_ClosestBSTValue.lean`
- `274` H-Index — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0274_HIndex.lean`
- `277` Find the Celebrity — relaxation (O(n)) — `lproofs/Lproofs/Problems/Fold/P0277_FindCelebrity.lean`
- `278` First Bad Version — bisection (O(log n)) — `lproofs/Lproofs/Problems/Bisection/P0278_FirstBadVersion.lean`
- `283` Move Zeroes — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0283_MoveZeroes.lean`
- `286` Walls and Gates — relaxation (O(mn)) — `lproofs/Lproofs/Problems/Relaxation/P0286_WallsAndGates.lean`
- `287` Find the Duplicate Number — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0287_FindDuplicateNumber.lean`
- `290` Word Pattern — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0290_WordPattern.lean`
- `297` Serialize and Deserialize Binary Tree — dp () — `lproofs/Lproofs/Problems/DP/P0297_SerializeDeserialize.lean`
- `303` Range Sum Query - Immutable — fold (O(1) query) — `lproofs/Lproofs/Problems/Fold/P0303_RangeSumQuery.lean`
- `305` Number of Islands II — relaxation (O(k·α)) — `lproofs/Lproofs/Problems/Relaxation/P0305_NumberOfIslandsII.lean`
- `309` Best Time to Buy and Sell Stock with Cooldown — dp () — `lproofs/Lproofs/Problems/DP/P0309_StockCooldown.lean`
- `312` Burst Balloons — dp (O(n³)) — `lproofs/Lproofs/Problems/DP/P0312_BurstBalloons.lean`
- `322` Coin Change — dp (O(amount·coins)) — `lproofs/Lproofs/Problems/DP/P0322_CoinChange.lean`
- `328` Odd Even Linked List — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0328_OddEvenLinkedList.lean`
- `329` Longest Increasing Path in a Matrix — dp () — `lproofs/Lproofs/Problems/Relaxation/P0329_LongestIncreasingPath.lean`
- `332` Reconstruct Itinerary — relaxation (O(V+E)) — `lproofs/Lproofs/Problems/Relaxation/P0332_Reach.lean`
- `344` Reverse String — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0344_ReverseString.lean`
- `349` Intersection of Two Arrays — fold (O(n+m)) — `lproofs/Lproofs/Problems/Fold/P0349_IntersectionOfTwoArrays.lean`
- `368` Largest Divisible Subset — dp (O(n²)) — `lproofs/Lproofs/Problems/DP/P0368_LargestDivisibleSubset.lean`
- `378` Kth Smallest Element in a Sorted Matrix — bisection () — `lproofs/Lproofs/Problems/Bisection/P0378_KthSmallestSortedMatrix.lean`
- `383` Ransom Note — fold (O(n+m)) — `lproofs/Lproofs/Problems/Fold/P0383_RansomNote.lean`
- `387` First Unique Character in a String — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0387_FirstUniqueCharacter.lean`
- `392` Is Subsequence — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0392_IsSubsequence.lean`
- `399` Evaluate Division — relaxation (O(Q·(V+E))) — `lproofs/Lproofs/Problems/Relaxation/P0399_EvaluateDivision.lean`
- `408` Valid Word Abbreviation — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0408_ValidWordAbbreviation.lean`
- `410` Split Array Largest Sum — bisection () — `lproofs/Lproofs/Problems/Bisection/P0410_SplitArrayLargestSum.lean`
- `415` Add Strings — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0415_AddStrings.lean`
- `416` Partition Equal Subset Sum — dp (O(n·sum)) — `lproofs/Lproofs/Problems/DP/P0416_PartitionEqualSubset.lean`
- `417` Pacific Atlantic Water Flow — relaxation () — `lproofs/Lproofs/Problems/Relaxation/P0417_PacificAtlantic.lean`
- `427` Construct Quad Tree — dp (O(n²)) — `lproofs/Lproofs/Problems/DP/P0427_QuadTree.lean`
- `442` Find All Duplicates in an Array — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0442_FindAllDuplicates.lean`
- `443` String Compression — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0443_StringCompression.lean`
- `448` Find All Numbers Disappeared in an Array — fold () — `lproofs/Lproofs/Problems/Fold/P0448_FindDisappearedNumbers.lean`
- `485` Max Consecutive Ones — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0485_MaxConsecutiveOnes.lean`
- `489` Robot Room Cleaner — dp (O(cells)) — `lproofs/Lproofs/Problems/Relaxation/P0489_RobotRoomCleaner.lean`
- `494` Target Sum — dp (O(n·sum)) — `lproofs/Lproofs/Problems/DP/P0494_TargetSum.lean`
- `505` The Maze II — relaxation (O(mn log mn)) — `lproofs/Lproofs/Problems/Relaxation/P0505_MazeII.lean`
- `509` Fibonacci Number — dp (O(n)) — `lproofs/Lproofs/Problems/DP/P0509_Fibonacci.lean`
- `516` Longest Palindromic Subsequence — dp (O(n²)) — `lproofs/Lproofs/Problems/DP/P0516_LongestPalindromicSubseq.lean`
- `528` Random Pick with Weight — bisection () — `lproofs/Lproofs/Problems/Bisection/P0528_RandomPickWeight.lean`
- `540` Single Element in a Sorted Array — bisection () — `lproofs/Lproofs/Problems/Bisection/P0540_SingleElementSorted.lean`
- `542` 01 Matrix — relaxation (O(V+E)) — `lproofs/Lproofs/Problems/Relaxation/P0542_01Matrix.lean`
- `543` Diameter of Binary Tree — dp (O(n)) — `lproofs/Lproofs/Problems/DP/P0543_DiameterOfBinaryTree.lean`
- `545` Boundary of Binary Tree — dp (O(n)) — `lproofs/Lproofs/Problems/DP/P0545_BoundaryOfTree.lean`
- `547` Number of Provinces — relaxation (O(V^2)) — `lproofs/Lproofs/Problems/Relaxation/P0547_NumberOfProvinces.lean`
- `549` Binary Tree Longest Consecutive Sequence II — dp () — `lproofs/Lproofs/Problems/DP/P0549_LongestConsecutiveII.lean`
- `560` Subarray Sum Equals K — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0560_SubarraySumK.lean`
- `567` Permutation in String — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0567_PermutationInString.lean`
- `611` Valid Triangle Number — fold (O(n²)) — `lproofs/Lproofs/Problems/Fold/P0611_ValidTriangle.lean`
- `636` Exclusive Time of Functions — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0636_ExclusiveTime.lean`
- `642` Design Search Autocomplete System — dp () — `lproofs/Lproofs/Problems/DP/P0642_SearchAutocomplete.lean`
- `648` Replace Words — dp (O(Σ) — `lproofs/Lproofs/Problems/DP/P0648_ReplaceWords.lean`
- `658` Find K Closest Elements — bisection (O(log n)) — `lproofs/Lproofs/Problems/Bisection/P0658_FindKClosest.lean`
- `662` Maximum Width of Binary Tree — relaxation () — `lproofs/Lproofs/Problems/Relaxation/P0662_MaximumWidthofBinaryTr.lean`
- `680` Valid Palindrome II — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0680_ValidPalindromeII.lean`
- `713` Subarray Product Less Than K — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0713_SubarrayProductLessThanK.lean`
- `714` Best Time to Buy and Sell Stock with Transaction Fee — dp () — `lproofs/Lproofs/Problems/DP/P0714_StockWithFee.lean`
- `719` Find K-th Smallest Pair Distance — bisection () — `lproofs/Lproofs/Problems/Bisection/P0719_KthSmallestPairDistance.lean`
- `724` Find Pivot Index — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0724_FindPivotIndex.lean`
- `733` Flood Fill — relaxation (O(n)) — `lproofs/Lproofs/Problems/Relaxation/P0733_FloodFill.lean`
- `735` Asteroid Collision — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0735_AsteroidCollision.lean`
- `778` Swim in Rising Water — relaxation (O(V log V)) — `lproofs/Lproofs/Problems/Relaxation/P0778_SwiminRisingWater.lean`
- `785` Is Graph Bipartite? — relaxation (O(V+E)) — `lproofs/Lproofs/Problems/Relaxation/P0785_IsGraphBipartite.lean`
- `787` Cheapest Flights Within K Stops — relaxation () — `lproofs/Lproofs/Problems/Relaxation/P0787_CheapestFlights.lean`
- `797` All Paths From Source to Target — relaxation () — `lproofs/Lproofs/Problems/DP/P0797_AllPathsSourceTarget.lean`
- `799` Champagne Tower — dp (O(r²)) — `lproofs/Lproofs/Problems/DP/P0799_ChampagneTower.lean`
- `815` Bus Routes — relaxation (O(V+E)) — `lproofs/Lproofs/Problems/Relaxation/P0815_BusRoutes.lean`
- `824` Goat Latin — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0824_GoatLatin.lean`
- `825` Friends Of Appropriate Ages — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0825_FriendsAppropriateAges.lean`
- `833` Find And Replace in String — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0833_FindReplaceString.lean`
- `852` Peak Index in a Mountain Array — bisection () — `lproofs/Lproofs/Problems/Bisection/P0852_PeakIndex.lean`
- `863` All Nodes Distance K in Binary Tree — relaxation () — `lproofs/Lproofs/Problems/Relaxation/P0863_AllNodesDistanceKinBinar.lean`
- `875` Koko Eating Bananas — bisection (O(n log m)) — `lproofs/Lproofs/Problems/Bisection/P0875_KokoBananas.lean`
- `889` Construct Binary Tree from Preorder and Postorder Traversal — dp () — `lproofs/Lproofs/Problems/DP/P0889_ConstructBTfromPreandPost.lean`
- `898` Bitwise ORs of Subarrays — dp (O(n·W)) — `lproofs/Lproofs/Problems/DP/P0898_BitwiseORsSubarrays.lean`
- `907` Sum of Subarray Minimums — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0907_SumSubarrayMinimums.lean`
- `921` Minimum Add to Make Parentheses Valid — fold () — `lproofs/Lproofs/Problems/Fold/P0921_MinAddParensValid.lean`
- `931` Minimum Falling Path Sum — dp (O(n²)) — `lproofs/Lproofs/Problems/DP/P0931_MinFallingPathSum.lean`
- `934` Shortest Bridge — relaxation (O(mn)) — `lproofs/Lproofs/Problems/Relaxation/P0934_ShortestBridge.lean`
- `941` Valid Mountain Array — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P0941_ValidMountainArray.lean`
- `959` Regions Cut By Slashes — relaxation (O(n^2)) — `lproofs/Lproofs/Problems/Relaxation/P0959_RegionsBySlashes.lean`
- `983` Minimum Cost For Tickets — dp (O(n)) — `lproofs/Lproofs/Problems/DP/P0983_MinCostTickets.lean`
- `986` Interval List Intersections — fold () — `lproofs/Lproofs/Problems/Fold/P0986_IntervalListIntersections.lean`
- `992` Subarrays with K Different Integers — fold () — `lproofs/Lproofs/Problems/Fold/P0992_SubarraysKDistinct.lean`
- `994` Rotting Oranges — relaxation (O(mn)) — `lproofs/Lproofs/Problems/Relaxation/P0994_RottingOranges.lean`
- `1004` Max Consecutive Ones III — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P1004_MaxConsecutiveOnesIII.lean`
- `1011` Capacity To Ship Packages Within D Days — bisection () — `lproofs/Lproofs/Problems/Bisection/P1011_ShipPackages.lean`
- `1047` Remove All Adjacent Duplicates In String — fold () — `lproofs/Lproofs/Problems/Fold/P1047_RemoveAdjacentDuplicates.lean`
- `1060` Missing Element in Sorted Array — bisection () — `lproofs/Lproofs/Problems/Bisection/P1060_MissingElementSorted.lean`
- `1064` Fixed Point — bisection (O(log n)) — `lproofs/Lproofs/Problems/Bisection/P1064_FixedPoint.lean`
- `1091` Shortest Path in Binary Matrix — relaxation (O(n²)) — `lproofs/Lproofs/Problems/Relaxation/P1091_ShortestPathBinaryMatrix.lean`
- `1094` Car Pooling — fold (O(n + maxPos)) — `lproofs/Lproofs/Problems/Fold/P1094_CarPooling.lean`
- `1166` Design File System — dp (O() — `lproofs/Lproofs/Problems/DP/P1166_DesignFileSystem.lean`
- `1171` Remove Zero Sum Consecutive Nodes from Linked List — fold () — `lproofs/Lproofs/Problems/Fold/P1171_RemoveZeroSumSublists.lean`
- `1202` Smallest String With Swaps — relaxation () — `lproofs/Lproofs/Problems/Relaxation/P1202_SmallestStringWithSwaps.lean`
- `1209` Remove All Adjacent Duplicates in String II — fold () — `lproofs/Lproofs/Problems/Fold/P1209_RemoveAdjacentDuplicatesII.lean`
- `1235` Maximum Profit in Job Scheduling — dp () — `lproofs/Lproofs/Problems/DP/P1235_MaxProfitJobScheduling.lean`
- `1347` Minimum Number of Steps to Make Two Strings Anagram — fold () — `lproofs/Lproofs/Problems/Fold/P1347_MinStepsAnagram.lean`
- `1368` Minimum Cost to Make at Least One Valid Path in a Grid — relaxation () — `lproofs/Lproofs/Problems/Relaxation/P1368_MinCostValidPath.lean`
- `1385` Find the Distance Value Between Two Arrays — bisection () — `lproofs/Lproofs/Problems/Fold/P1385_DistanceValueBetweenTwoArrays.lean`
- `1423` Maximum Points You Can Obtain from Cards — fold () — `lproofs/Lproofs/Problems/Fold/P1423_MaxPointsCards.lean`
- `1428` Leftmost Column with at Least a One — bisection () — `lproofs/Lproofs/Problems/Bisection/P1428_LeftmostColumnWithOne.lean`
- `1438` Longest Continuous Subarray With Absolute Diff ≤ Limit — fold () — `lproofs/Lproofs/Problems/Fold/P1438_LongestSubarrayLimit.lean`
- `1443` Minimum Time to Collect All Apples in a Tree — dp () — `lproofs/Lproofs/Problems/DP/P1443_CollectApples.lean`
- `1462` Course Schedule IV — relaxation (O(V*E)) — `lproofs/Lproofs/Problems/Relaxation/P1462_CourseScheduleIV.lean`
- `1466` Reorder Routes to Make All Paths Lead to the City Zero — relaxation () — `lproofs/Lproofs/Problems/Relaxation/P1466_ReorderRoutestoMakeAll.lean`
- `1480` Running Sum of 1d Array — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P1480_RunningSum.lean`
- `1482` Minimum Number of Days to Make m Bouquets — bisection () — `lproofs/Lproofs/Problems/Bisection/P1482_MinDaysBouquets.lean`
- `1492` The kth Factor of n — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P1492_KthFactorOfN.lean`
- `1494` Parallel Courses II — dp (O(3^n)) — `lproofs/Lproofs/Problems/DP/P1494_ParallelCoursesII.lean`
- `1539` Kth Missing Positive Number — bisection () — `lproofs/Lproofs/Problems/Bisection/P1539_KthMissingPositive.lean`
- `1584` Min Cost to Connect All Points — relaxation () — `lproofs/Lproofs/Problems/Relaxation/P1584_MinCostConnectPoints.lean`
- `1614` Maximum Nesting Depth of the Parentheses — fold () — `lproofs/Lproofs/Problems/Fold/P1614_MaxNestingDepth.lean`
- `1648` Sell Diminishing-Valued Colored Balls — bisection () — `lproofs/Lproofs/Problems/Bisection/P1648_SellDiminishingBalls.lean`
- `1657` Determine if Two Strings Are Close — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P1657_DetermineCloseStrings.lean`
- `1673` Find the Most Competitive Subsequence — fold () — `lproofs/Lproofs/Problems/Fold/P1673_MostCompetitiveSubsequence.lean`
- `1691` Maximum Height by Stacking Cuboids — dp () — `lproofs/Lproofs/Problems/DP/P1691_MaxHeightStackingCuboids.lean`
- `1719` Number Of Ways To Reconstruct A Tree — relaxation () — `lproofs/Lproofs/Problems/Relaxation/P1719_Reach.lean`
- `1768` Merge Strings Alternately — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P1768_MergeAlternately.lean`
- `1778` Shortest Path in a Hidden Grid — relaxation (O(mn)) — `lproofs/Lproofs/Problems/Relaxation/P1778_ShortestPathHiddenGrid.lean`
- `1818` Minimum Absolute Sum Difference — bisection () — `lproofs/Lproofs/Problems/Bisection/P1818_MinAbsSumDiff.lean`
- `1823` Find the Winner of the Circular Game — fold () — `lproofs/Lproofs/Problems/Fold/P1823_FindTheWinnerCircularGame.lean`
- `1856` Maximum Subarray Min-Product — fold () — `lproofs/Lproofs/Problems/Fold/P1856_MaxSubarrayMinProduct.lean`
- `1871` Jump Game VII — dp () — `lproofs/Lproofs/Problems/Relaxation/P1871_JumpGameVII.lean`
- `1971` Find if Path Exists in Graph — relaxation () — `lproofs/Lproofs/Problems/Relaxation/P1971_FindPathExists.lean`
- `2050` Parallel Courses III — relaxation (O(V+E)) — `lproofs/Lproofs/Problems/Relaxation/P2050_ParallelCoursesIII.lean`
- `2092` Find All People With Secret — relaxation () — `lproofs/Lproofs/Problems/Relaxation/P2092_Reach.lean`
- `2150` Find All Lonely Numbers in the Array — fold () — `lproofs/Lproofs/Problems/Fold/P2150_FindAllLonelyNumbers.lean`
- `2187` Minimum Time to Complete Trips — bisection () — `lproofs/Lproofs/Problems/Bisection/P2187_MinTimeToCompleteTrips.lean`
- `2214` Minimum Health to Beat Game — dp (O(n)) — `lproofs/Lproofs/Problems/Fold/P2214_MinHealth.lean`
- `2258` Escape the Spreading Fire — relaxation (O(V+E)) — `lproofs/Lproofs/Problems/Relaxation/P2258_EscapetheSpreadingFire.lean`
- `2282` Number of People That Can Be Seen in a Grid — fold () — `lproofs/Lproofs/Problems/Fold/P2282_PeopleSeenInGrid.lean`
- `2337` Move Pieces to Obtain a String — fold () — `lproofs/Lproofs/Problems/Fold/P2337_MovePieces.lean`
- `2385` Amount of Time for Binary Tree to Be Infected — relaxation () — `lproofs/Lproofs/Problems/Relaxation/P2385_AmountofTimeforBinaryTre.lean`
- `2444` Count Subarrays With Fixed Bounds — fold () — `lproofs/Lproofs/Problems/Fold/P2444_CountSubarraysFixedBounds.lean`
- `2493` Divide Nodes Into the Maximum Number of Groups — relaxation () — `lproofs/Lproofs/Problems/Relaxation/P2493_DivideNodesMaxGroups.lean`
- `2503` Maximum Number of Points From Grid Queries — relaxation () — `lproofs/Lproofs/Problems/Relaxation/P2503_MaxPointsGridQueries.lean`
- `2661` First Completely Painted Row or Column — fold () — `lproofs/Lproofs/Problems/Fold/P2661_FirstCompletelyPainted.lean`
- `2858` Minimum Edge Reversals for All Reachable — relaxation () — `lproofs/Lproofs/Problems/Relaxation/P2858_Reach.lean`
- `2861` Maximum Number of Alloys — bisection () — `lproofs/Lproofs/Problems/Bisection/P2861_MaximumNumberOfAlloys.lean`
- `2912` Number of Ways to Reach Destination in the Grid — dp () — `lproofs/Lproofs/Problems/DP/P2912_NumberOfWaysGrid.lean`
- `2965` Find Missing and Repeated Values — fold (O(n²)) — `lproofs/Lproofs/Problems/Fold/P2965_FindMissingAndRepeated.lean`
- `3043` Find the Length of the Longest Common Prefix — dp () — `lproofs/Lproofs/Problems/DP/P3043_LongestCommonPrefix.lean`
- `3120` Count the Number of Special Characters I — fold () — `lproofs/Lproofs/Problems/Fold/P3120_CountSpecialCharacters.lean`
- `3129` Find All Possible Stable Binary Arrays I — dp () — `lproofs/Lproofs/Problems/DP/P3129_StableBinaryArrays.lean`
- `3161` Block Placement Queries — bisection (O(log n)) — `lproofs/Lproofs/Problems/Bisection/P3161_Bisect.lean`
- `3371` Identify the Largest Outlier in an Array — fold () — `lproofs/Lproofs/Problems/Fold/P3371_LargestOutlier.lean`
- `3387` Graph Reachability — relaxation (O(V+E)) — `lproofs/Lproofs/Problems/Relaxation/P3387_Reach.lean`
- `3453` Separate Squares I — bisection (O(n log)) — `lproofs/Lproofs/Problems/Bisection/P3453_SeparateSquares.lean`
- `3629` Minimum Jumps to Reach End via Prime Teleportation — relaxation () — `lproofs/Lproofs/Problems/Relaxation/P3629_PrimeTeleportation.lean`
- `3637` Trionic Array I — fold (O(n)) — `lproofs/Lproofs/Problems/Fold/P3637_TrionicArray.lean`
- `3741` Minimum Distance Between Three Equal Elements II — fold () — `lproofs/Lproofs/Problems/Fold/P3741_MinDistThreeEqual.lean`
- `3928` Minimum Cost to Buy Apples II — relaxation (O(V+E)) — `lproofs/Lproofs/Problems/Relaxation/P3928_MinimumCosttoBuyApples.lean`
