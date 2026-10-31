export default function LintIssues(props: { issues: Array<{ severity: string; message: string }> }) {
  return (
    <ul class="empty:hidden space-y-2 rounded-lg border border-amber-300 bg-amber-50 p-4 text-sm text-amber-950 dark:border-amber-900 dark:bg-amber-950 dark:text-amber-100">
      {props.issues.map((issue) => <li><strong>{issue.severity}:</strong> {issue.message}</li>)}
    </ul>
  );
}
