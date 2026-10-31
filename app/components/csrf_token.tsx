export default function CsrfToken(props: { token: string }) {
  return <input type="hidden" name="_csrf" value={props.token} />;
}
